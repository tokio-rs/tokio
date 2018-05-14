//! Unix Domain Sockets for Tokio.
//!
//! This crate provides APIs for using Unix Domain Sockets with Tokio.

#![cfg(unix)]
#![doc(html_root_url = "https://docs.rs/tokio-uds/0.2.0")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate libc;
extern crate log;
extern crate mio;
extern crate mio_uds;
extern crate tokio_io;
extern crate tokio_reactor;

mod datagram;
mod incoming;
mod listener;
mod recv_dgram;
mod send_dgram;
mod stream;
mod ucred;

pub use datagram::UnixDatagram;
pub use incoming::Incoming;
pub use listener::UnixListener;
pub use recv_dgram::RecvDgram;
pub use send_dgram::SendDgram;
pub use stream::UnixStream;
pub use ucred::UCred;
