#![cfg(unix)]
#![doc(html_root_url = "https://docs.rs/tokio-uds/0.3.0-alpha.1")]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

//! Unix Domain Sockets for Tokio.
//!
//! This crate provides APIs for using Unix Domain Sockets with Tokio.

mod datagram;
// mod frame;
mod incoming;
mod listener;
mod stream;
mod ucred;

pub use crate::datagram::UnixDatagram;
#[cfg(feature = "async-traits")]
pub use crate::incoming::Incoming;
pub use crate::listener::UnixListener;
pub use crate::stream::UnixStream;
pub use crate::ucred::UCred;
