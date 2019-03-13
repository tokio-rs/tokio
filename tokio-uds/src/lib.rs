#![cfg(unix)]
#![doc(html_root_url = "https://docs.rs/tokio-uds/0.2.5")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

//! Unix Domain Sockets for Tokio.
//!
//! This crate provides APIs for using Unix Domain Sockets with Tokio.

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate libc;
#[macro_use]
extern crate log;
extern crate mio;
extern crate mio_uds;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_reactor;

mod datagram;
mod frame;
mod frame_connected;
mod incoming;
mod listener;
mod recv_dgram2;
mod recv_dgram_from;
mod send_dgram2;
mod send_dgram_to;
mod stream;
mod ucred;

pub use datagram::UnixDatagram;
pub use frame::UnixDatagramFramed;
pub use frame_connected::UnixDatagramConnectedFramed;
pub use incoming::Incoming;
pub use listener::UnixListener;
pub use recv_dgram2::RecvDgram2;
pub use recv_dgram_from::RecvDgramFrom;
#[deprecated(since = "0.2.6", note = "use RecvDgramFrom instead")]
#[doc(hidden)]
pub use recv_dgram_from::RecvDgramFrom as RecvDgram;
pub use send_dgram2::SendDgram2;
pub use send_dgram_to::SendDgramTo;
#[deprecated(since = "0.2.6", note = "use SendDgramTo instead")]
#[doc(hidden)]
pub use send_dgram_to::SendDgramTo as SendDgram;
pub use stream::{ConnectFuture, UnixStream};
pub use ucred::UCred;
