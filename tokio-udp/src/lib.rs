#![doc(html_root_url = "https://docs.rs/tokio-tcp/0.1.5")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

//! UDP bindings for `tokio`.
//!
//! This module contains the UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! The main struct for UDP is the [`UdpSocket`], which represents a UDP socket.
//! Reading and writing to it can be done using futures, which return the
//! [`RecvDgram`] and [`SendDgram`] structs respectively.
//!
//! For convenience it's also possible to convert raw datagrams into higher-level
//! frames.
//!
//! [`UdpSocket`]: struct.UdpSocket.html
//! [`RecvDgram`]: struct.RecvDgram.html
//! [`SendDgram`]: struct.SendDgram.html
//! [`UdpFramed`]: struct.UdpFramed.html
//! [`framed`]: struct.UdpSocket.html#method.framed

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate mio;
#[macro_use]
extern crate log;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_reactor;

mod frame;
mod recv_dgram;
mod send_dgram;
mod socket;

pub use self::frame::UdpFramed;
pub use self::recv_dgram::RecvDgram;
pub use self::send_dgram::SendDgram;
pub use self::socket::UdpSocket;
