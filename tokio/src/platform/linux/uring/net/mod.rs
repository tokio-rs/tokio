//! TCP/UDP bindings for `tokio-uring`.
//!
//! This module contains the TCP/UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! # Organization
//!
//! * [`TcpListener`] and [`TcpStream`] provide functionality for communication over TCP
//! * [`UdpSocket`] provides functionality for communication over UDP

//!
//! [`TcpListener`]: TcpListener
//! [`TcpStream`]: TcpStream
//! [`UdpSocket`]: UdpSocket

mod tcp;
mod udp;
mod unix;

pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
pub use unix::{UnixListener, UnixStream};
