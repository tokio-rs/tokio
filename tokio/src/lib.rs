#![doc(html_root_url = "https://docs.rs/tokio/0.1.20")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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
//! ```no_run,ignore
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

macro_rules! if_runtime {
    ($($i:item)*) => ($(
        #[cfg(any(feature = "rt-full"))]
        $i
    )*)
}

#[cfg(feature = "timer")]
pub mod clock;
#[cfg(feature = "codec")]
pub mod codec;
#[cfg(feature = "fs")]
pub mod fs;
#[cfg(feature = "io")]
pub mod io;
#[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
pub mod net;
pub mod prelude;
#[cfg(feature = "reactor")]
pub mod reactor;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "timer")]
pub mod timer;
//pub mod util;

if_runtime! {
    pub mod executor;
    pub mod runtime;

    pub use crate::executor::spawn;
    pub use crate::runtime::run;
}
