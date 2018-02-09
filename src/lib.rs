//! `Future`-powered I/O at the core of Tokio
//!
//! This crate uses the [`futures`] crate to provide an event loop ("reactor
//! core") which can be used to drive I/O like TCP and UDP. All asynchronous I/O
//! is powered by the [`mio`] crate.
//!
//! [`futures`]: ../futures/index.html
//! [`mio`]: ../mio/index.html
//!
//! The concrete types provided in this crate are relatively bare bones but are
//! intended to be the essential foundation for further projects needing an
//! event loop. In this crate you'll find:
//!
//! * TCP, both streams and listeners.
//! * UDP sockets.
//! * An event loop to run futures.
//!
//! More functionality is likely to be added over time, but otherwise the crate
//! is intended to be flexible, with the [`PollEvented`] type accepting any
//! type that implements [`mio::Evented`]. For example, the [`tokio-uds`] crate
//! uses [`PollEvented`] to provide support for Unix domain sockets.
//!
//! [`PollEvented`]: ./reactor/struct.PollEvented.html
//! [`mio::Evented`]: ../mio/event/trait.Evented.html
//! [`tokio-uds`]: https://crates.io/crates/tokio-uds
//!
//! Some other important tasks covered by this crate are:
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
//! extern crate futures_cpupool;
//! extern crate tokio;
//! extern crate tokio_io;
//!
//! use futures::prelude::*;
//! use futures::future::Executor;
//! use futures_cpupool::CpuPool;
//! use tokio_io::AsyncRead;
//! use tokio_io::io::copy;
//! use tokio::net::TcpListener;
//!
//! fn main() {
//!     let pool = CpuPool::new_num_cpus();
//!
//!     // Bind the server's socket.
//!     let addr = "127.0.0.1:12345".parse().unwrap();
//!     let listener = TcpListener::bind(&addr)
//!         .expect("unable to bind TCP listener");
//!
//!     // Pull out a stream of sockets for incoming connections
//!     let server = listener.incoming().for_each(|sock| {
//!         // Split up the reading and writing parts of the
//!         // socket.
//!         let (reader, writer) = sock.split();
//!
//!         // A future that echos the data and returns how
//!         // many bytes were copied...
//!         let bytes_copied = copy(reader, writer);
//!
//!         // ... after which we'll print what happened.
//!         let handle_conn = bytes_copied.map(|amt| {
//!             println!("wrote {:?} bytes", amt)
//!         }).map_err(|err| {
//!             eprintln!("IO error {:?}", err)
//!         });
//!
//!         // Spawn the future as a concurrent task.
//!         pool.execute(handle_conn).unwrap();
//!
//!         Ok(())
//!     });
//!
//!     // Spin up the server on this thread
//!     server.wait().unwrap();
//! }
//! ```

#![doc(html_root_url = "https://docs.rs/tokio/0.1.1")]
#![deny(missing_docs)]
#![deny(warnings)]
#![warn(missing_debug_implementations)]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate mio;
extern crate slab;
#[macro_use]
extern crate tokio_io;

#[macro_use]
extern crate log;

pub mod executor;
pub mod net;
pub mod reactor;
