#![allow(dead_code, unused_variables, unreachable_pub)]
use std::{collections, io, net, num, sync};
mod machine;
pub mod tcp;
use machine::{LogicalMachine, LogicalMachineId};
pub(crate) mod time;
mod util;

#[derive(Debug)]
pub(self) struct State {
    next_machine_id: usize,
    id_to_machine: collections::HashMap<LogicalMachineId, LogicalMachine>,
    hostname_to_machineid: collections::HashMap<String, LogicalMachineId>,
    ipaddr_to_machineid: collections::HashMap<net::IpAddr, LogicalMachineId>,
    time: time::SimTime,
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
            time: time::SimTime::new(),
        };
        let inner = sync::Arc::new(sync::Mutex::new(state));
        let mut simulation = Self { inner };
        simulation.register("localhost");
        simulation
    }

    // Register a new logical machine with the simulation.
    fn register<T>(&mut self, hostname: T) -> MachineContext
    where
        T: std::string::ToString,
    {
        let mut lock = self.inner.lock().unwrap();
        let id = LogicalMachineId::new(lock.next_machine_id);
        lock.next_machine_id += 1;
        let machine_ipaddr = lock.unused_ipaddr();
        let machine = LogicalMachine::new(id, hostname.to_string(), machine_ipaddr);
        lock.id_to_machine.insert(id, machine);
        lock.hostname_to_machineid.insert(hostname.to_string(), id);
        lock.ipaddr_to_machineid.insert(machine_ipaddr, id);
        MachineContext {
            machineid: id,
            inner: sync::Arc::clone(&self.inner),
        }
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
    pub(crate) fn now(&self) -> crate::time::Instant {
        let lock = self.inner.lock().unwrap();
        lock.time.now()
    }

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
        let mut lock = self.inner.lock().unwrap();
        let machine = lock
            .id_to_machine
            .get_mut(&LogicalMachineId::new(0))
            .expect("could not find associated logical machine");
        machine.bind_listener(port)
    }

    pub async fn connect(&self, addr: std::net::SocketAddr) -> io::Result<tcp::SimTcpStream> {
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
            let client_ipaddr = lock
                .id_to_machine
                .get(&LogicalMachineId::new(0))
                .unwrap()
                .ipaddr();
            let client_addr = net::SocketAddr::new(client_ipaddr, 9999);
            let machine = lock.id_to_machine.get_mut(&machineid).unwrap();
            machine.connect(client_addr, port)
        };
        fut.await
    }
}

pub struct MachineContext {
    machineid: LogicalMachineId,
    inner: sync::Arc<sync::Mutex<State>>,
}

impl MachineContext {
    /// Bind a new TcpListener to the provided machineid port. A port value of 0
    /// will bind to a random port.
    ///
    /// Returns a TcpListener if the machineid is present and binding is successful.
    pub fn bind(&self, port: u16) -> io::Result<tcp::SimTcpListener> {
        let mut lock = self.inner.lock().unwrap();
        let machine = lock
            .id_to_machine
            .get_mut(&self.machineid)
            .expect("could not find associated logical machine");
        machine.bind_listener(port)
    }

    pub async fn connect(&self, addr: std::net::SocketAddr) -> io::Result<tcp::SimTcpStream> {
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
            let client_ipaddr = lock.id_to_machine.get(&self.machineid).unwrap().ipaddr();
            let client_addr = net::SocketAddr::new(client_ipaddr, 9999);
            let machine = lock.id_to_machine.get_mut(&machineid).unwrap();
            machine.connect(client_addr, port)
        };
        fut.await
    }
}
