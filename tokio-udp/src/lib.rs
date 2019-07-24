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
//! [`Recv`], [`Send`], [`RecvFrom`] and [`SendTo`] structs respectively.

// mod frame;
mod recv;
mod recv_from;
mod send;
mod send_to;
mod socket;
pub mod split;

// pub use self::frame::UdpFramed;
pub use self::recv::Recv;
pub use self::recv_from::RecvFrom;
pub use self::send::Send;
pub use self::send_to::SendTo;

pub use self::socket::UdpSocket;
