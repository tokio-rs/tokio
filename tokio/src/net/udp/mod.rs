//! UDP bindings for `tokio`.
//!
//! This module contains the UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! The main struct for UDP is the [`UdpSocket`], which represents a UDP socket.
//!
//! [`UdpSocket`]: struct.UdpSocket

mod socket;
pub use socket::UdpSocket;

mod split;
pub use split::{RecvHalf, SendHalf, ReuniteError};
