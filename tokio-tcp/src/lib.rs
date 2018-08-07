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

#![doc(html_root_url = "https://docs.rs/tokio-tcp/0.1.1")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate mio;
extern crate tokio_io;
extern crate tokio_reactor;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

mod incoming;
mod listener;
mod stream;

pub use self::incoming::Incoming;
pub use self::listener::TcpListener;
pub use self::stream::TcpStream;
pub use self::stream::ConnectFuture;

#[cfg(feature = "unstable-futures")]
fn lift_async<T>(old: futures::Async<T>) -> futures2::Async<T> {
    match old {
        futures::Async::Ready(x) => futures2::Async::Ready(x),
        futures::Async::NotReady => futures2::Async::Pending,
    }
}

#[cfg(feature = "unstable-futures")]
fn lower_async<T>(new: futures2::Async<T>) -> futures::Async<T> {
    match new {
        futures2::Async::Ready(x) => futures::Async::Ready(x),
        futures2::Async::Pending => futures::Async::NotReady,
    }
}
