#![cfg(not(loom))]

//! TCP/UDP/Unix bindings for `tokio`.
//!
//! This module contains the TCP/UDP/Unix networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! # Organization
//!
//! * [`TcpListener`] and [`TcpStream`] provide functionality for communication over TCP
//! * [`UdpSocket`] provides functionality for communication over UDP
//! * [`UnixListener`] and [`UnixStream`] provide functionality for communication over a
//! Unix Domain Stream Socket **(available on Unix only)**
//! * [`UnixDatagram`] provides functionality for communication
//! over Unix Domain Datagram Socket **(available on Unix only)**

//!
//! [`TcpListener`]: TcpListener
//! [`TcpStream`]: TcpStream
//! [`UdpSocket`]: UdpSocket
//! [`UnixListener`]: UnixListener
//! [`UnixStream`]: UnixStream
//! [`UnixDatagram`]: UnixDatagram

mod addr;
pub use addr::ToSocketAddrs;

cfg_dns! {
    mod lookup_host;
    pub use lookup_host::lookup_host;
}

cfg_tcp! {
    pub mod tcp;
    pub use tcp::listener::TcpListener;
    pub use tcp::stream::TcpStream;
}

cfg_udp! {
    pub mod udp;
    pub use udp::socket::UdpSocket;
}

cfg_uds! {
    pub mod unix;
    pub use unix::datagram::UnixDatagram;
    pub use unix::listener::UnixListener;
    pub use unix::stream::UnixStream;
}
