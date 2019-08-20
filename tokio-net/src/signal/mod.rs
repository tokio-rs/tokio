//! Asynchronous signal handling for Tokio
//!
//! The primary type exported from this crate, `unix::Signal`, allows
//! listening for arbitrary signals on Unix platforms, receiving them
//! in an asynchronous fashion.
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
//! use tokio_net::signal;
//!
//! use futures_util::future;
//! use futures_util::stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create an infinite stream of "Ctrl+C" notifications. Each item received
//!     // on this stream may represent multiple ctrl-c signals.
//!     let ctrl_c = signal::ctrl_c()?;
//!
//!     // Process each ctrl-c as it comes in
//!     let prog = ctrl_c.for_each(|_| {
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
//! # #[cfg(unix)] {
//!
//! use tokio_net::signal::{self, unix::{signal, SignalKind}};
//!
//! use futures_util::future;
//! use futures_util::stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create an infinite stream of "Ctrl+C" notifications. Each item received
//!     // on this stream may represent multiple ctrl-c signals.
//!     let ctrl_c = signal::ctrl_c()?;
//!
//!     // Process each ctrl-c as it comes in
//!     let prog = ctrl_c.for_each(|_| {
//!         println!("ctrl-c received!");
//!         future::ready(())
//!     });
//!
//!     prog.await;
//!
//!     // Like the previous example, this is an infinite stream of signals
//!     // being received, and signals may be coalesced while pending.
//!     let stream = signal(SignalKind::hangup())?;
//!
//!     // Convert out stream into a future and block the program
//!     let (signal, _stream) = stream.into_future().await;
//!     println!("got signal {:?}", signal);
//!     Ok(())
//! }
//! # }
//! ```

mod ctrl_c;
mod registry;

mod os {
    #[cfg(unix)]
    pub(crate) use super::unix::{OsExtraData, OsStorage};
    #[cfg(windows)]
    pub(crate) use super::windows::{OsExtraData, OsStorage};
}

pub mod unix;
pub mod windows;

pub use self::ctrl_c::{ctrl_c, CtrlC};
