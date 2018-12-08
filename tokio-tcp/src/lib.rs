#![doc(html_root_url = "https://docs.rs/tokio-tcp/0.1.2")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

//! TCP bindings for `tokio`.
//!
//! This module contains the TCP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! Connecting to an address, via TCP, can be done using [`TcpStream`]'s
//! [`connect`] method, which returns [`ConnectFuture`]. `ConnectFuture`
//! implements a future which returns a `TcpStream`.
//!
//! To listen on an address [`TcpListener`] can be used. `TcpListener`'s
//! [`incoming`][incoming_method] method can be used to accept new connections.
//! It return the [`Incoming`] struct, which implements a stream which returns
//! `TcpStream`s.
//!
//! [`TcpStream`]: struct.TcpStream.html
//! [`connect`]: struct.TcpStream.html#method.connect
//! [`ConnectFuture`]: struct.ConnectFuture.html
//! [`TcpListener`]: struct.TcpListener.html
//! [incoming_method]: struct.TcpListener.html#method.incoming
//! [`Incoming`]: struct.Incoming.html

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate mio;
extern crate tokio_io;
extern crate tokio_reactor;

mod incoming;
mod listener;
mod stream;

pub use self::incoming::Incoming;
pub use self::listener::TcpListener;
pub use self::stream::TcpStream;
pub use self::stream::ConnectFuture;
