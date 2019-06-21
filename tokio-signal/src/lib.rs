#![doc(html_root_url = "https://docs.rs/tokio-signal/0.2.8")]
#![deny(missing_docs, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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
//! use futures::{Future, Stream};
//!
//! // Create an infinite stream of "Ctrl+C" notifications. Each item received
//! // on this stream may represent multiple ctrl-c signals.
//! let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
//!
//! // Process each ctrl-c as it comes in
//! let prog = ctrl_c.for_each(|()| {
//!     println!("ctrl-c received!");
//!     Ok(())
//! });
//!
//! tokio::runtime::current_thread::block_on_all(prog).unwrap();
//! ```
//!
//! Wait for SIGHUP on Unix
//!
//! ```rust,no_run
//! # #[cfg(unix)] fn dox() {
//! use futures::{Future, Stream};
//! use tokio_signal::unix::{Signal, SIGHUP};
//!
//! // Like the previous example, this is an infinite stream of signals
//! // being received, and signals may be coalesced while pending.
//! let stream = Signal::new(SIGHUP).flatten_stream();
//!
//! // Convert out stream into a future and block the program
//! tokio::runtime::current_thread::block_on_all(stream.into_future()).ok().unwrap();
//! # }
//! ```

use futures::stream::Stream;
use futures::{future, Future};
use std::io;
use tokio_reactor::Handle;

pub mod unix;
pub mod windows;

/// A future whose error is `io::Error`
pub type IoFuture<T> = Box<dyn Future<Item = T, Error = io::Error> + Send>;
/// A stream whose error is `io::Error`
pub type IoStream<T> = Box<dyn Stream<Item = T, Error = io::Error> + Send>;

/// Creates a stream which receives "ctrl-c" notifications sent to a process.
///
/// In general signals are handled very differently across Unix and Windows, but
/// this is somewhat cross platform in terms of how it can be handled. A ctrl-c
/// event to a console process can be represented as a stream for both Windows
/// and Unix.
///
/// This function binds to the default event loop. Note that
/// there are a number of caveats listening for signals, and you may wish to
/// read up on the documentation in the `unix` or `windows` module to take a
/// peek.
pub fn ctrl_c() -> IoFuture<IoStream<()>> {
    ctrl_c_handle(&Handle::default())
}

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
pub fn ctrl_c_handle(handle: &Handle) -> IoFuture<IoStream<()>> {
    return ctrl_c_imp(handle);

    #[cfg(unix)]
    fn ctrl_c_imp(handle: &Handle) -> IoFuture<IoStream<()>> {
        let handle = handle.clone();
        Box::new(future::lazy(move || {
            unix::Signal::with_handle(unix::libc::SIGINT, &handle)
                .map(|x| Box::new(x.map(|_| ())) as Box<dyn Stream<Item = _, Error = _> + Send>)
        }))
    }

    #[cfg(windows)]
    fn ctrl_c_imp(handle: &Handle) -> IoFuture<IoStream<()>> {
        let handle = handle.clone();
        // Use lazy to ensure that `ctrl_c` gets called while on an event loop
        Box::new(future::lazy(move || {
            windows::Event::ctrl_c_handle(&handle)
                .map(|x| Box::new(x) as Box<dyn Stream<Item = _, Error = _> + Send>)
        }))
    }
}
