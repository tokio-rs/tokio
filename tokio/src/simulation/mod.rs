#![allow(dead_code, unused_variables, unreachable_pub)]
use crate::runtime::context;
use std::{collections, io, net, num, string::ToString, sync};
mod machine;
pub mod tcp;
mod util;
use crate::task::JoinHandle;
use machine::LogicalMachine;
pub(crate) use machine::SimTask;
pub use machine::{current_machineid, LogicalMachineId};
use std::future::Future;

#[derive(Debug)]
pub(self) struct State {
    next_machine_id: usize,
    id_to_machine: collections::HashMap<LogicalMachineId, LogicalMachine>,
    hostname_to_machineid: collections::HashMap<String, LogicalMachineId>,
    ipaddr_to_machineid: collections::HashMap<net::IpAddr, LogicalMachineId>,
}

impl State {
    /// Iterates through all logical machines looking for an unused ip address.
    fn unused_ipaddr(&mut self) -> net::IpAddr {
        let ipaddrs = self
            .id_to_machine
            .values()
            .map(|v| v.ipaddr())
            .collect::<Vec<_>>();
        util::find_unused_ipaddr(&ipaddrs)
    }
}

impl State {
    fn register_machine<T>(&mut self, hostname: T) -> LogicalMachineId
    where
        T: ToString,
    {
        let id = LogicalMachineId::new(self.next_machine_id);
        self.next_machine_id += 1;
        let machine_ipaddr = self.unused_ipaddr();
        let machine = LogicalMachine::new(id, hostname.to_string(), machine_ipaddr);
        self.id_to_machine.insert(id, machine);
        self.hostname_to_machineid.insert(hostname.to_string(), id);
        self.ipaddr_to_machineid.insert(machine_ipaddr, id);
        id
    }
}

/// Contains all state for a simulation run.
#[derive(Debug)]
pub struct Simulation {
    inner: sync::Arc<sync::Mutex<State>>,
}

impl Simulation {
    pub fn new() -> Self {
        let state = State {
            next_machine_id: 0,
            id_to_machine: collections::HashMap::new(),
            hostname_to_machineid: collections::HashMap::new(),
            ipaddr_to_machineid: collections::HashMap::new(),
        };
        let inner = sync::Arc::new(sync::Mutex::new(state));
        let mut simulation = Self { inner };
        // register a default logical machine at LogicalMachineId 0;
        let machineid = simulation.register_machine("localhost");
        machine::set_current_machineid(machineid);
        simulation
    }

    // Register a new logical machine with the simulation.
    fn register_machine<T>(&mut self, hostname: T) -> LogicalMachineId
    where
        T: ToString,
    {
        let mut lock = self.inner.lock().unwrap();
        lock.register_machine(hostname)
    }

    pub fn handle(&self) -> SimulationHandle {
        let inner = sync::Arc::clone(&self.inner);
        SimulationHandle { inner }
    }
}

#[derive(Debug, Clone)]
pub struct SimulationHandle {
    inner: sync::Arc<sync::Mutex<State>>,
}

impl SimulationHandle {
    pub fn resolve_tuple(
        &self,
        &(addr, port): &(&str, u16),
    ) -> Result<std::vec::IntoIter<net::SocketAddr>, io::Error> {
        let lock = self.inner.lock().unwrap();
        let target_machineid = lock.hostname_to_machineid.get(addr).ok_or(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "host could not be reached",
        ))?;
        let machine = lock
            .id_to_machine
            .get(target_machineid)
            .expect("expected machine set to never be modified");
        let target_ipaddr = machine.ipaddr();
        let target_socketaddr = net::SocketAddr::new(target_ipaddr.into(), port);
        Ok(vec![target_socketaddr].into_iter())
    }

    /// Bind a new TcpListener to the provided machineid port. A port value of 0
    /// will bind to a random port.
    ///
    /// Returns a TcpListener if the machineid is present and binding is successful.
    pub fn bind(&self, port: u16) -> io::Result<tcp::SimTcpListener> {
        let current_machineid = current_machineid();
        let mut lock = self.inner.lock().unwrap();
        let machine = lock
            .id_to_machine
            .get_mut(&current_machineid)
            .expect("could not find associated logical machine");
        machine.bind_listener(port)
    }

    pub async fn connect(&self, addr: std::net::SocketAddr) -> io::Result<tcp::SimTcpStream> {
        let current_machineid = current_machineid();

        let (ipaddr, port) = (addr.ip(), addr.port());
        let machineid = {
            let lock = self.inner.lock().unwrap();
            lock.ipaddr_to_machineid
                .get(&ipaddr)
                .ok_or(io::ErrorKind::ConnectionReset)
                .map(|v| *v)
        }?;

        // TODO: Use the correct error for a 0 port
        let port = num::NonZeroU16::new(port).ok_or(io::ErrorKind::InvalidInput)?;

        let fut = {
            let mut lock = self.inner.lock().unwrap();
            let target_machine = lock.id_to_machine.get(&current_machineid).unwrap();
            let machine = lock.id_to_machine.get_mut(&machineid).unwrap();
            machine.connect(port)
        };
        fut.await
    }

    fn register_machine<T>(&self, hostname: T) -> LogicalMachineId
    where
        T: ToString,
    {
        let mut lock = self.inner.lock().unwrap();
        lock.register_machine(hostname)
    }
}

pub fn spawn_machine<T, F>(hostname: T, f: F) -> JoinHandle<F::Output>
where
    T: ToString,
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handle = context::simulation_handle()
        .expect("cannot spawn simulation machine from outside of a runtime context");
    let machineid = handle.register_machine(hostname);
    let wrap = machine::SimTask::new(f, machineid);
    crate::spawn(wrap)
}

#[cfg(test)]
mod test {
    use crate::io::AsyncReadExt;
    use crate::io::AsyncWriteExt;
    use crate::simulation::spawn_machine;
    use std::error::Error;
    use std::io;

    async fn hello_server(port: u16) -> io::Result<()> {
        let mut listener = crate::net::TcpListener::bind(("0.0.0.0", port)).await?;
        let (mut conn, _) = listener.accept().await?;
        conn.write_all("hello".as_bytes()).await?;
        Ok(())
    }

    #[test]
    fn simulation_time() -> Result<(), Box<dyn Error>> {
        let mut runtime = crate::runtime::Builder::new()
            .simulated_runtime(0)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            // Test that a 30 second timeout elapses instantly, this also allows
            // the future spawned above to run until idle, setting up the network
            // binding.
            let time_before = crate::time::Instant::now();
            crate::time::delay_for(std::time::Duration::from_secs(30)).await;
            let time_after = crate::time::Instant::now();

            assert!(
                time_before + std::time::Duration::from_secs(30) >= time_after,
                "expected at least 30s to elapse"
            );
            Ok(())
        })
    }

    #[test]
    fn simulation_networking() -> Result<(), Box<dyn Error>> {
        let mut runtime = crate::runtime::Builder::new()
            .simulated_runtime(0)
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            // Spawn a server
            let server_port = 9092;
            spawn_machine("server", hello_server(server_port));
            // wait for the server to come up
            crate::time::delay_for(std::time::Duration::from_secs(30)).await;

            // Attempt to connect over the simulated network to the server spawned above,
            // reading the "hello" response.
            let mut stream = crate::net::TcpStream::connect(("server", server_port)).await?;
            let mut target = vec![0; 5];
            stream.read_exact(&mut target[..]).await?;

            let result = String::from_utf8(target)?;
            assert_eq!(String::from("hello"), result);
            Ok(())
        })
    }

    #[test]
    fn simulation_spawn_inherits_machine() -> Result<(), Box<dyn Error>> {
        let mut runtime = crate::runtime::Builder::new()
            .simulated_runtime(0)
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let jh = spawn_machine("parent", async {
                let parent_machine_id = crate::simulation::current_machineid();
                let jh = crate::spawn(async move {
                    let child_machineid = crate::simulation::current_machineid();
                    assert_eq!(parent_machine_id, child_machineid)
                })
                .await;
            })
            .await;
        });
        Ok(())
    }
}
