//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * A multi threaded, work-stealing based task [scheduler][runtime].
//! * A [reactor][reactor] backed by the operating system's event queue (epoll, kqueue,
//!   IOCP, etc...).
//! * Asynchronous [TCP and UDP][net] sockets.
//!
//! Tokio is built using futures (provided by the [futures] crate) as the
//! abstraction for managing the complexity of asynchronous programming.
//!
//! Guide level documentation is found on the [website].
//!
//! [website]: https://tokio.rs/docs/getting-started/hello-world/
//! [futures]: http://docs.rs/futures
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

#![doc(html_root_url = "https://docs.rs/tokio/0.1.1")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate iovec;
extern crate mio;
extern crate slab;
extern crate tokio_io;
extern crate tokio_executor;
extern crate tokio_reactor;
extern crate tokio_threadpool;

#[macro_use]
extern crate log;

pub mod executor;
pub mod net;
pub mod reactor;
pub mod runtime;

pub use executor::spawn;
pub use runtime::run;

pub mod io {
    //! Asynchronous I/O.
    //!
    //! This module is the asynchronous version of `std::io`. Primarily, it
    //! defines two traits, [`AsyncRead`] and [`AsyncWrite`], which extend the
    //! `Read` and `Write` traits of the standard library.
    //!
    //! [`AsyncRead`] and [`AsyncWrite`] must only be implemented for
    //! non-blocking I/O types that integrate with the futures type system. In
    //! other words, these types must never block the thread, and instead the
    //! current task is notified when the I/O resource is ready.
    //!
    //! Utilities functions are provided for working with [`AsyncRead`] /
    //! [`AsyncWrite`] types. For example, [`copy`] asynchronously copies all
    //! data from a source to a destination.
    //!
    //! Additionally, [`Read`], [`Write`], [`Error`], [`ErrorKind`], and
    //! [`Result`] are re-exported from `std::io` for ease of use.
    //!
    //! [`AsyncRead`]: trait.AsyncRead.html
    //! [`AsyncWrite`]: trait.AsyncWrite.html
    //! [`copy`]: fn.copy.html
    //! [`Read`]: trait.Read.html
    //! [`Write`]: trait.Write.html
    //! [`Error`]: struct.Error.html
    //! [`ErrorKind`]: enum.ErrorKind.html
    //! [`Result`]: type.Result.html

    pub use tokio_io::{
        AsyncRead,
        AsyncWrite,
    };

    // Utils
    pub use tokio_io::io::{
        copy,
        Copy,
        flush,
        Flush,
        lines,
        Lines,
        read_exact,
        ReadExact,
        read_to_end,
        ReadToEnd,
        read_until,
        ReadUntil,
        shutdown,
        Shutdown,
        write_all,
        WriteAll,
    };

    // Re-export io::Error so that users don't have to deal
    // with conflicts when `use`ing `futures::io` and `std::io`.
    pub use ::std::io::{
        Error,
        ErrorKind,
        Result,
        Read,
        Write,
    };
}

pub mod prelude {
    //! A "prelude" for users of the `tokio` crate.
    //!
    //! This prelude is similar to the standard library's prelude in that you'll
    //! almost always want to import its entire contents, but unlike the standard
    //! library's prelude you'll have to do so manually:
    //!
    //! ```
    //! use tokio::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use tokio_io::{
        AsyncRead,
        AsyncWrite,
    };

    pub use ::std::io::{
        Read,
        Write,
    };

    pub use futures::{
        Future,
        future,
        Stream,
        stream,
        Sink,
        IntoFuture,
        Async,
        AsyncSink,
        Poll,
        task,
    };
}
