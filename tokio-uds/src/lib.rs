#![cfg(unix)]
#![doc(html_root_url = "https://docs.rs/tokio-uds/0.2.7")]
#![deny(missing_docs, missing_debug_implementations)]

//! Unix Domain Sockets for Tokio.
//!
//! > **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved
//! > into [`tokio::net`] behind the `uds` [feature flag].
//!
//! [`tokio::net`]: https://docs.rs/tokio/latest/tokio/net/index.html
//! [feature flag]: https://docs.rs/tokio/latest/tokio/index.html#feature-flags
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
mod incoming;
mod listener;
mod recv_dgram;
mod send_dgram;
mod stream;
mod ucred;

pub use datagram::UnixDatagram;
pub use frame::UnixDatagramFramed;
pub use incoming::Incoming;
pub use listener::UnixListener;
pub use recv_dgram::RecvDgram;
pub use send_dgram::SendDgram;
pub use stream::{ConnectFuture, UnixStream};
pub use ucred::UCred;
