#![doc(html_root_url = "https://docs.rs/tokio/0.1.11")]
#![deny(missing_docs, warnings, missing_debug_implementations)]
#![cfg_attr(feature = "async-await-preview", feature(
        async_await,
        await_macro,
        futures_api,
        ))]

//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * A multi threaded, work-stealing based task [scheduler][runtime].
//! * A [reactor] backed by the operating system's event queue (epoll, kqueue,
//!   IOCP, etc...).
//! * Asynchronous [TCP and UDP][net] sockets.
//! * Asynchronous [filesystem][fs] operations.
//! * [Timer][timer] API for scheduling work in the future.
//!
//! Tokio is built using [futures] as the abstraction for managing the
//! complexity of asynchronous programming.
//!
//! Guide level documentation is found on the [website].
//!
//! [website]: https://tokio.rs/docs/getting-started/hello-world/
//! [futures]: http://docs.rs/futures/0.1
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! extern crate tokio;
//!
//! use tokio::prelude::*;
//! use tokio::io::copy;
//! use tokio::net::TcpListener;
//!
//! fn main() {
//!     // Bind the server's socket.
//!     let addr = "127.0.0.1:12345".parse().unwrap();
//!     let listener = TcpListener::bind(&addr)
//!         .expect("unable to bind TCP listener");
//!
//!     // Pull out a stream of sockets for incoming connections
//!     let server = listener.incoming()
//!         .map_err(|e| eprintln!("accept failed = {:?}", e))
//!         .for_each(|sock| {
//!             // Split up the reading and writing parts of the
//!             // socket.
//!             let (reader, writer) = sock.split();
//!
//!             // A future that echos the data and returns how
//!             // many bytes were copied...
//!             let bytes_copied = copy(reader, writer);
//!
//!             // ... after which we'll print what happened.
//!             let handle_conn = bytes_copied.map(|amt| {
//!                 println!("wrote {:?} bytes", amt)
//!             }).map_err(|err| {
//!                 eprintln!("IO error {:?}", err)
//!             });
//!
//!             // Spawn the future as a concurrent task.
//!             tokio::spawn(handle_conn)
//!         });
//!
//!     // Start the Tokio runtime
//!     tokio::run(server);
//! }
//! ```

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate mio;
extern crate tokio_current_thread;
extern crate tokio_io;
extern crate tokio_executor;
extern crate tokio_codec;
extern crate tokio_fs;
extern crate tokio_reactor;
extern crate tokio_threadpool;
extern crate tokio_timer;
extern crate tokio_tcp;
extern crate tokio_udp;

#[cfg(feature = "async-await-preview")]
extern crate tokio_async_await;

#[cfg(unix)]
extern crate tokio_uds;

pub mod clock;
pub mod codec;
pub mod executor;
pub mod fs;
pub mod io;
pub mod net;
pub mod prelude;
pub mod reactor;
pub mod runtime;
pub mod timer;
pub mod util;

pub use executor::spawn;
pub use runtime::run;

// ===== Experimental async/await support =====

#[cfg(feature = "async-await-preview")]
mod async_await;

#[cfg(feature = "async-await-preview")]
pub use async_await::{run_async, spawn_async};

#[cfg(feature = "async-await-preview")]
pub use tokio_async_await::await;
