//! TCP/UDP bindings for `tokio-core`
//!
//! This module contains the TCP/UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.

mod tcp;
mod udp;

pub use self::tcp::{TcpStream, TcpStreamNew};
pub use self::tcp::{Incoming, TcpListener};
pub use self::udp::{RecvDgram, SendDgram, UdpCodec, UdpFramed, UdpSocket};
