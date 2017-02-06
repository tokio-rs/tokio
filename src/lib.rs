//! `Future`-powered I/O at the core of Tokio
//!
//! This crate uses the `futures` crate to provide an event loop ("reactor
//! core") which can be used to drive I/O like TCP and UDP, spawned future
//! tasks, and other events like channels/timeouts. All asynchronous I/O is
//! powered by the `mio` crate.
//!
//! The concrete types provided in this crate are relatively bare bones but are
//! intended to be the essential foundation for further projects needing an
//! event loop. In this crate you'll find:
//!
//! * TCP, both streams and listeners
//! * UDP sockets
//! * Timeouts
//! * An event loop to run futures
//!
//! More functionality is likely to be added over time, but otherwise the crate
//! is intended to be flexible, with the `PollEvented` type accepting any
//! type that implements `mio::Evented`. For example, the `tokio-uds` crate
//! uses `PollEvented` to provide support for Unix domain sockets.
//!
//! Some other important tasks covered by this crate are:
//!
//! * The ability to spawn futures into an event loop. The `Handle` and `Remote`
//!   types have a `spawn` method which allows executing a future on an event
//!   loop. The `Handle::spawn` method crucially does not require the future
//!   itself to be `Send`.
//!
//! * The `Io` trait serves as an abstraction for future crates to build on top
//!   of. This packages up `Read` and `Write` functionality as well as the
//!   ability to poll for readiness on both ends.
//!
//! * All I/O is futures-aware. If any action in this crate returns "not ready"
//!   or "would block", then the current future task is scheduled to receive a
//!   notification when it would otherwise make progress.
//!
//! You can find more extensive documentation in terms of tutorials at
//! [https://tokio.rs](https://tokio.rs).
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//!
//! use futures::{Future, Stream};
//! use tokio_core::io::{copy, Io};
//! use tokio_core::net::TcpListener;
//! use tokio_core::reactor::Core;
//!
//! fn main() {
//!     // Create the event loop that will drive this server
//!     let mut core = Core::new().unwrap();
//!     let handle = core.handle();
//!
//!     // Bind the server's socket
//!     let addr = "127.0.0.1:12345".parse().unwrap();
//!     let listener = TcpListener::bind(&addr, &handle).unwrap();
//!
//!     // Pull out a stream of sockets for incoming connections
//!     let server = listener.incoming().for_each(|(sock, _)| {
//!         // Split up the reading and writing parts of the
//!         // socket
//!         let (reader, writer) = sock.split();
//!
//!         // A future that echos the data and returns how
//!         // many bytes were copied...
//!         let bytes_copied = copy(reader, writer);
//!
//!         // ... after which we'll print what happened
//!         let handle_conn = bytes_copied.map(|amt| {
//!             println!("wrote {} bytes", amt)
//!         }).map_err(|err| {
//!             println!("IO error {:?}", err)
//!         });
//!
//!         // Spawn the future as a concurrent task
//!         handle.spawn(handle_conn);
//!
//!         Ok(())
//!     });
//!
//!     // Spin up the server on the event loop
//!     core.run(server).unwrap();
//! }
//! ```

#![doc(html_root_url = "https://docs.rs/tokio-core/0.1")]
#![deny(missing_docs)]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate mio;
extern crate slab;
extern crate tokio_io;

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate log;

#[macro_use]
pub mod io;

mod heap;
#[doc(hidden)]
pub mod channel;
pub mod net;
pub mod reactor;

use std::io as sio;

fn would_block() -> sio::Error {
    sio::Error::new(sio::ErrorKind::WouldBlock, "would block")
}
