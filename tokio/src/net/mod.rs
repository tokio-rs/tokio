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

pub mod driver;

pub mod util;

#[cfg(feature = "tcp")]
pub mod tcp;

#[cfg(feature = "tcp")]
pub use self::tcp::{TcpListener, TcpStream};

#[cfg(feature = "udp")]
pub mod udp;

#[cfg(feature = "udp")]
pub use self::udp::UdpSocket;

#[cfg(all(unix, feature = "uds"))]
pub mod unix;

#[cfg(all(unix, feature = "uds"))]
pub use self::unix::{UnixDatagram, UnixListener, UnixStream};
