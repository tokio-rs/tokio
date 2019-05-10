#![doc(html_root_url = "https://docs.rs/tokio-tcp/0.1.3")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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

mod frame;
mod recv_dgram;
mod send_dgram;
mod socket;

pub use self::frame::UdpFramed;
pub use self::recv_dgram::RecvDgram;
pub use self::send_dgram::SendDgram;
pub use self::socket::UdpSocket;
