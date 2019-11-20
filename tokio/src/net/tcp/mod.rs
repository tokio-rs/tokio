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

mod listener;
pub use listener::TcpListener;

mod incoming;
pub use incoming::Incoming;

mod split;
pub use split::{ReadHalf, WriteHalf};

mod stream;
pub use stream::TcpStream;
