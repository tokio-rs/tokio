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
#[cfg(feature = "net")]
pub(crate) use addr::to_socket_addrs;
pub use addr::ToSocketAddrs;

cfg_net! {
    mod lookup_host;
    pub use lookup_host::lookup_host;

    pub mod tcp;
    pub use tcp::listener::TcpListener;
    pub use tcp::socket::TcpSocket;
    pub use tcp::stream::TcpStream;

    mod udp;
    pub use udp::UdpSocket;
}

cfg_net_unix! {
    pub mod unix;
    pub use unix::datagram::socket::UnixDatagram;
    pub use unix::listener::UnixListener;
    pub use unix::stream::UnixStream;
}

cfg_net_windows! {
    pub mod windows;
}
