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
//! * [`UnixDatagram`] and [`UnixDatagramFramed`] provide functionality for communication
//! over Unix Domain Datagram Socket **(available on Unix only)**

//!
//! [`TcpListener`]: struct.TcpListener.html
//! [`TcpStream`]: struct.TcpStream.html
//! [`UdpSocket`]: struct.UdpSocket.html
//! [`UnixListener`]: struct.UnixListener.html
//! [`UnixStream`]: struct.UnixStream.html
//! [`UnixDatagram`]: struct.UnixDatagram.html
//! [`UnixDatagramFramed`]: struct.UnixDatagramFramed.html

mod addr;
pub use addr::ToSocketAddrs;

cfg_tcp! {
    pub mod tcp;
    pub use tcp::{TcpListener, TcpStream};
}

cfg_udp! {
    pub mod udp;
    pub use udp::UdpSocket;
}

cfg_uds! {
    pub mod unix;
    pub use unix::{UnixDatagram, UnixListener, UnixStream};
}
