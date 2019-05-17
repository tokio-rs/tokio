#![cfg(unix)]
#![doc(html_root_url = "https://docs.rs/tokio-uds/0.2.5")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Unix Domain Sockets for Tokio.
//!
//! This crate provides APIs for using Unix Domain Sockets with Tokio.

mod datagram;
mod frame;
mod incoming;
mod listener;
mod recv_dgram;
mod send_dgram;
mod stream;
mod ucred;

pub use crate::datagram::UnixDatagram;
pub use crate::frame::UnixDatagramFramed;
pub use crate::incoming::Incoming;
pub use crate::listener::UnixListener;
pub use crate::recv_dgram::RecvDgram;
pub use crate::send_dgram::SendDgram;
pub use crate::stream::{ConnectFuture, UnixStream};
pub use crate::ucred::UCred;
