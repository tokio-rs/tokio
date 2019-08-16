//! UDP bindings for `tokio`.
//!
//! This module contains the UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! The main struct for UDP is the [`UdpSocket`], which represents a UDP socket.
//! Reading and writing to it can be done using futures, which return the
//! [`Recv`], [`Send`], [`RecvFrom`] and [`SendTo`] structs respectively.

mod frame;
mod socket;
pub mod split;

pub use self::frame::UdpFramed;
pub use self::socket::UdpSocket;
