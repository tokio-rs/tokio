#![doc(html_root_url = "https://docs.rs/tokio-tcp/0.1.3")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

//! TCP bindings for `tokio`.
//!
//! This module contains the TCP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! Connecting to an address, via TCP, can be done using [`TcpStream`]'s
//! [`connect`] method, which returns a future which returns a `TcpStream`.
//!
//! To listen on an address [`TcpListener`] can be used. `TcpListener`'s
//! [`incoming`][incoming_method] method can be used to accept new connections.
//! It return the [`Incoming`] struct, which implements a stream which returns
//! `TcpStream`s.
//!
//! [`TcpStream`]: struct.TcpStream.html
//! [`connect`]: struct.TcpStream.html#method.connect
//! [`TcpListener`]: struct.TcpListener.html
//! [incoming_method]: struct.TcpListener.html#method.incoming
//! [`Incoming`]: struct.Incoming.html

#[cfg(feature = "incoming")]
mod incoming;
mod listener;
pub mod split;
mod stream;

pub use self::listener::TcpListener;
pub use self::stream::TcpStream;
