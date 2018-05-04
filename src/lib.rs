//! Asynchronous signal handling for Tokio
//!
//! This crate implements asynchronous signal handling for Tokio, an
//! asynchronous I/O framework in Rust. The primary type exported from this
//! crate, `unix::Signal`, allows listening for arbitrary signals on Unix
//! platforms, receiving them in an asynchronous fashion.
//!
//! Note that signal handling is in general a very tricky topic and should be
//! used with great care. This crate attempts to implement 'best practice' for
//! signal handling, but it should be evaluated for your own applications' needs
//! to see if it's suitable.
//!
//! The are some fundamental limitations of this crate documented on the
//! `Signal` structure as well.
//!
//! # Examples
//!
//! Print out all ctrl-C notifications received
//!
//! ```rust,no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_signal;
//!
//! use tokio_core::reactor::Core;
//! use futures::{Future, Stream};
//!
//! fn main() {
//!     let mut core = Core::new().unwrap();
//!     let handle = core.handle();
//!     let handle = handle.new_tokio_handle();
//!
//!     // Create an infinite stream of "Ctrl+C" notifications. Each item received
//!     // on this stream may represent multiple ctrl-c signals.
//!     let ctrl_c = tokio_signal::ctrl_c(&handle).flatten_stream();
//!
//!     // Process each ctrl-c as it comes in
//!     let prog = ctrl_c.for_each(|()| {
//!         println!("ctrl-c received!");
//!         Ok(())
//!     });
//!
//!     core.run(prog).unwrap();
//! }
//! ```
//!
//! Wait for SIGHUP on Unix
//!
//! ```rust,no_run
//! # extern crate futures;
//! # extern crate tokio_core;
//! # extern crate tokio_signal;
//! # #[cfg(unix)]
//! # mod foo {
//! #
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_signal;
//!
//! use tokio_core::reactor::Core;
//! use futures::{Future, Stream};
//! use tokio_signal::unix::{Signal, SIGHUP};
//!
//! fn main() {
//!     let mut core = Core::new().unwrap();
//!     let handle = core.handle();
//!     let handle = handle.new_tokio_handle();
//!
//!     // Like the previous example, this is an infinite stream of signals
//!     // being received, and signals may be coalesced while pending.
//!     let stream = Signal::new(SIGHUP, &handle).flatten_stream();
//!
//!     // Convert out stream into a future and block the program
//!     core.run(stream.into_future()).ok().unwrap();
//! }
//! # }
//! # fn main() {}
//! ```

#![doc(html_root_url = "https://docs.rs/tokio-signal/0.1")]
#![deny(missing_docs)]

extern crate futures;
extern crate mio;
extern crate tokio_executor;
extern crate tokio_io;
extern crate tokio_reactor;

use std::io;

use futures::stream::Stream;
use futures::{future, Future};
use tokio_reactor::Handle;

pub mod unix;
pub mod windows;

/// A future whose error is `io::Error`
pub type IoFuture<T> = Box<Future<Item = T, Error = io::Error> + Send>;
/// A stream whose error is `io::Error`
pub type IoStream<T> = Box<Stream<Item = T, Error = io::Error> + Send>;

/// Creates a stream which receives "ctrl-c" notifications sent to a process.
///
/// In general signals are handled very differently across Unix and Windows, but
/// this is somewhat cross platform in terms of how it can be handled. A ctrl-c
/// event to a console process can be represented as a stream for both Windows
/// and Unix.
///
/// This function receives a `Handle` to an event loop and returns a future
/// which when resolves yields a stream receiving all signal events. Note that
/// there are a number of caveats listening for signals, and you may wish to
/// read up on the documentation in the `unix` or `windows` module to take a
/// peek.
pub fn ctrl_c(handle: &Handle) -> IoFuture<IoStream<()>> {
    return ctrl_c_imp(handle);

    #[cfg(unix)]
    fn ctrl_c_imp(handle: &Handle) -> IoFuture<IoStream<()>> {
        let handle = handle.clone();
        Box::new(future::lazy(move || {
            unix::Signal::new(unix::libc::SIGINT, &handle)
                .map(|x| Box::new(x.map(|_| ())) as Box<Stream<Item = _, Error = _> + Send>)
        }))
    }

    #[cfg(windows)]
    fn ctrl_c_imp(handle: &Handle) -> IoFuture<IoStream<()>> {
        let handle = handle.clone();
        // Use lazy to ensure that `ctrl_c` gets called while on an event loop
        Box::new(future::lazy(move || {
            windows::Event::ctrl_c(&handle)
                .map(|x| Box::new(x) as Box<Stream<Item = _, Error = _> + Send>)
        }))
    }
}
