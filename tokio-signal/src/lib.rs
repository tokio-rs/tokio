#![doc(html_root_url = "https://docs.rs/tokio-signal/0.2.8")]
#![deny(missing_docs, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![cfg_attr(test, feature(async_await))]
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
//! #![feature(async_await)]
//!
//! use futures_util::future;
//! use futures_util::stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create an infinite stream of "Ctrl+C" notifications. Each item received
//!     // on this stream may represent multiple ctrl-c signals.
//!     let ctrl_c = tokio_signal::ctrl_c().await?;
//!
//!     // Process each ctrl-c as it comes in
//!     let prog = ctrl_c.for_each(|event| {
//!         event.expect("failed to get event");
//!
//!         println!("ctrl-c received!");
//!         future::ready(())
//!     });
//!
//!     prog.await;
//!
//!     Ok(())
//! }
//! ```
//!
//! Wait for SIGHUP on Unix
//!
//! ```rust,no_run
//! #![feature(async_await)]
//!
//! use futures_util::future;
//! use futures_util::stream::StreamExt;
//! use tokio_signal::unix::{Signal, SIGHUP};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create an infinite stream of "Ctrl+C" notifications. Each item received
//!     // on this stream may represent multiple ctrl-c signals.
//!     let ctrl_c = tokio_signal::ctrl_c().await?;
//!
//!     // Process each ctrl-c as it comes in
//!     let prog = ctrl_c.for_each(|event| {
//!         event.expect("failed to get event");
//!
//!         println!("ctrl-c received!");
//!         future::ready(())
//!     });
//!
//!     prog.await;
//!
//!     // Like the previous example, this is an infinite stream of signals
//!     // being received, and signals may be coalesced while pending.
//!     let stream = Signal::new(SIGHUP).await?;
//!
//!     // Convert out stream into a future and block the program
//!     let (signal, _signal) = stream.into_future().await;
//!     println!("got signal {:?}", signal);
//!     Ok(())
//! }
//! ```

#[macro_use]
extern crate lazy_static;

use futures_core::future::Future;
use futures_core::stream::Stream;
use futures_util::future::FutureExt;
use futures_util::stream::StreamExt;
use futures_util::try_future::TryFutureExt;
use std::io;
use std::pin::Pin;
use tokio_reactor::Handle;

mod registry;

mod os {
    #[cfg(unix)]
    pub(crate) use super::unix::{OsExtraData, OsStorage};
    #[cfg(windows)]
    pub(crate) use super::windows::{OsExtraData, OsStorage};
}

pub mod unix;
pub mod windows;

/// A future whose output is `io::Result<T>`
pub type IoFuture<T> = Pin<Box<dyn Future<Output = io::Result<T>> + Send>>;
/// A stream whose item is `io::Result<T>`
pub type IoStream<T> = Pin<Box<dyn Stream<Item = io::Result<T>> + Send>>;

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
        unix::Signal::with_handle(unix::libc::SIGINT, &handle)
            .map_ok(|signal| -> IoStream<()> { signal.map(|_| Ok(())).boxed() })
            .boxed()
    }

    #[cfg(windows)]
    fn ctrl_c_imp(handle: &Handle) -> IoFuture<IoStream<()>> {
        windows::Event::ctrl_c_handle(&handle)
            .map_ok(|event| -> IoStream<()> { event.map(|_| Ok(())).boxed() })
            .boxed()
    }
}
