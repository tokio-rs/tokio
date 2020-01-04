//! Simulation state for a logical machine in a simulation.

use crate::simulation::tcp::{
    SimTclListenerHandle, SimTcpListener, SimTcpStream, SimTcpStreamHandle,
};
use std::future::Future;
use std::{collections, io, net, num, pin::Pin, string};

/// LogicalMachineId is a token used to tie spawned tasks to a particular logical machine.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LogicalMachineId(usize);
impl LogicalMachineId {
    pub(crate) fn new(id: usize) -> Self {
        Self(id)
    }
}

#[derive(Debug)]
pub(crate) struct LogicalMachine {
    id: LogicalMachineId,
    hostname: String,
    ipaddr: net::IpAddr,
    tags: collections::HashMap<String, String>,
    acceptors: collections::HashMap<num::NonZeroU16, SimTclListenerHandle>,
    connections: Vec<SimTcpStreamHandle>,
}

impl LogicalMachine {
    pub(crate) fn new<T>(id: LogicalMachineId, hostname: T, ipaddr: net::IpAddr) -> Self
    where
        T: string::ToString,
    {
        Self {
            id,
            hostname: hostname.to_string(),
            ipaddr: ipaddr,
            tags: collections::HashMap::new(),
            acceptors: collections::HashMap::new(),
            connections: vec![],
        }
    }

    pub(crate) fn hostname(&self) -> &str {
        self.hostname.as_ref()
    }

    pub(crate) fn ipaddr(&self) -> net::IpAddr {
        self.ipaddr
    }

    pub(crate) fn bind_listener(&mut self, port: u16) -> Result<SimTcpListener, io::Error> {
        self.gc_connections();
        let port = self.allocate_port(port)?;
        let (acceptor, handle) =
            SimTcpListener::new(net::SocketAddr::new(self.ipaddr(), port.get()));
        self.acceptors.insert(port, handle);
        Ok(acceptor)
    }

    pub(crate) fn connect(
        &mut self,
        client_addr: net::SocketAddr,
        port: num::NonZeroU16,
    ) -> Pin<Box<dyn Future<Output = Result<SimTcpStream, io::Error>> + Send + 'static>> {
        self.gc_connections();
        let acceptor = self
            .acceptors
            .get(&port)
            .cloned()
            .ok_or(io::ErrorKind::ConnectionRefused.into());
        match acceptor {
            Ok(mut acceptor) => {
                let server_addr = net::SocketAddr::new(self.ipaddr(), port.get());
                let (client, server, handle) = SimTcpStream::new_pair(client_addr, server_addr);
                self.connections.push(handle);
                Box::pin(async move {
                    acceptor.enqueue_incoming(server).await?;
                    Ok(client)
                })
            }
            Err(e) => Box::pin(async { Err(e) }),
        }
    }

    fn allocate_port(&mut self, port: u16) -> Result<num::NonZeroU16, io::Error> {
        let mut candidate_port = port;
        loop {
            if let Some(valid_port) = num::NonZeroU16::new(candidate_port) {
                if self.acceptors.contains_key(&valid_port) {
                    return Err(io::ErrorKind::AddrInUse.into());
                } else {
                    return Ok(valid_port);
                }
            } else {
                candidate_port = u16::checked_add(candidate_port, 1).ok_or(io::Error::new(
                    io::ErrorKind::Other,
                    format!("no more ports available for machine {}", self.hostname),
                ))?;
            }
        }
    }

    fn gc_connections(&mut self) {
        self.acceptors.retain(|_, v| !v.dropped());
        self.connections.retain(|v| !v.dropped());
    }
}
