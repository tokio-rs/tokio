#![doc(html_root_url = "https://docs.rs/tokio/0.2.4")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * Tools for [working with asynchronous tasks][tasks], including
//!   [synchronization primitives and channels][sync] and [timeouts, delays, and
//!   intervals][time].
//! * APIs for [performing asynchronous I/O][io], including [TCP and UDP][net] sockets,
//!   [filesystem][fs] operations, and [process] and [signal] management.
//! * A [runtime] for executing asynchronous code, including a task scheduler,
//!   an I/O driver backed by the operating system's event queue (epoll, kqueue,
//!   IOCP, etc...), and a high performance timer.
//!
//! Guide level documentation is found on the [website].
//!
//! [tasks]: #working-with-tasks
//! [sync]: crate::sync
//! [time]: crate::time
//! [io]: #asynchronous-io
//! [net]: crate::net
//! [fs]: crate::fs
//! [process]: crate::process
//! [signal]: crate::signal
//! [fs]: crate::fs
//! [runtime]: crate::runtime
//! [website]: https://tokio.rs/docs/
//!
//! # A Tour of Tokio
//!
//! Tokio consists of a number of modules that provide a range of functionality
//! essential for implementing asynchronous applications in Rust. In this
//! section, we will take a brief tour of Tokio, summarizing the major APIs and
//! their uses.
//!
//! Note that Tokio uses [Cargo feature flags][features] to allow users to
//! control what features are present, so that unused code can be eliminated.
//! This documentation also lists what feature flags are necessary to enable each API.
//!
//! The easiest way to get started is to enable all features. Do this by
//! enabling the `full` feature flag:
//!
//! ```toml
//! tokio = { version = "0.2", features = ["full"] }
//! ```
//!
//! [features]: https://doc.rust-lang.org/cargo/reference/manifest.html#the-features-section
//!
//! ## Working With Tasks
//!
//! Asynchronous programs in Rust are based around lightweight, non-blocking
//! units of execution called [_tasks_][tasks]. The [`tokio::task`] module provides
//! important tools for working with tasks:
//!
//! * The [`spawn`] function and [`JoinHandle`] type, for scheduling a new task
//!   on the Tokio runtime and awaiting the output of a spawned task, respectively,
//! * Functions for [running blocking operations][blocking] in an asynchronous
//!   task context.
//!
//! The [`tokio::task`] module is present only when the "rt-core" feature flag
//! is enabled.
//!
//! [tasks]: task/index.html#what-are-tasks
//! [`tokio::task`]: crate::task
//! [`spawn`]: crate::task::spawn()
//! [`JoinHandle`]: crate::task::JoinHandle
//! [blocking]: task/index.html#blocking-and-yielding
//!
//! The [`tokio::sync`] module contains synchronization primitives to use when
//! need to communicate or share data. These include:
//!
//! * channels ([`oneshot`], [`mpsc`], and [`watch`]), for sending values
//!   between tasks,
//! * a non-blocking [`Mutex`], for controlling access to a shared, mutable
//!   value,
//! * an asynchronous [`Barrier`] type, for multiple tasks to synchronize before
//!   beginning a computation.
//!
//! The `tokio::sync` module is present only when the "sync" feature flag is
//! enabled.
//!
//! [`tokio::sync`]: crate::sync
//! [`Mutex`]: crate::sync::Mutex
//! [`Barrier`]: crate::sync::Barrier
//! [`oneshot`]: crate::sync::oneshot
//! [`mpsc`]: crate::sync::mpsc
//! [`watch`]: crate::sync::watch
//!
//! The [`tokio::time`] module provides utilities for tracking time and
//! scheduling work. This includes functions for setting [timeouts][timeout] for
//! tasks, [delaying][delay] work to run in the future, or [repeating an operation at an
//! interval][interval].
//!
//! In order to use `tokio::time`, the "time" feature flag must be enabled.
//!
//! [`tokio::time`]: crate::time
//! [delay]: crate::time::delay_for()
//! [interval]: crate::time::interval()
//! [timeout]: crate::time::timeout()
//!
//! Finally, Tokio provides a _runtime_ for executing asynchronous tasks. Most
//! applications can use the [`#[tokio::main]`][main] macro to run their code on the
//! Tokio runtime. In use-cases where manual control over the runtime is
//! required, the [`tokio::runtime`] module provides APIs for configuring and
//! managing runtimes.
//!
//! Using the runtime requires the "rt-core" or "rt-threaded" feature flags, to
//! enable the basic [single-threaded scheduler][rt-core] and the [thread-pool
//! scheduler][rt-threaded], respectively. See the [`runtime` module
//! documentation][rt-features] for details. In addition, the "macros" feature
//! flag enables the `#[tokio::main]` and `#[tokio::test]` attributes.
//!
//! [main]: attr.main.html
//! [`tokio::runtime`]: crate::runtime
//! [`Builder`]: crate::runtime::Builder
//! [`Runtime`]: crate::runtime::Runtime
//! [rt-core]: runtime/index.html#basic-scheduler
//! [rt-threaded]: runtime/index.html#threaded-scheduler
//! [rt-features]: runtime/index.html#runtime-scheduler
//!
//! ## Asynchronous IO
//!
//! As well as scheduling and running tasks, Tokio provides everything you need
//! to perform input and output asynchronously.
//!
//! The [`tokio::io`] module provides Tokio's asynchronous core I/O primitives,
//! the [`AsyncRead`], [`AsyncWrite`], and [`AsyncBufRead`] traits. In addition,
//! when the "io-util" feature flag is enabled, it also provides combinators and
//! functions for working with these traits, forming as an asynchronous
//! counterpart to [`std::io`]. When the "io-driver" feature flag is enabled, it
//! also provides utilities for library authors implementing I/O resources.
//!
//! Tokio also includes APIs for performing various kinds of I/O and interacting
//! with the operating system asynchronously. These include:
//!
//! * [`tokio::net`], which contains non-blocking versions of [TCP], [UDP], and
//!   [Unix Domain Sockets][UDS] (enabled by the "net" feature flag),
//! * [`tokio::fs`], similar to [`std::fs`] but for performing filesystem I/O
//!   asynchronously (enabled by the "fs" feature flag),
//! * [`tokio::signal`], for asynchronously handling Unix and Windows OS signals
//!   (enabled by the "signal" feature flag),
//! * [`tokio::process`], for spawning and managing child processes (enabled by
//!   the "process" feature flag).
//!
//! [`tokio::io`]: crate::io
//! [`AsyncRead`]: crate::io::AsyncRead
//! [`AsyncWrite`]: crate::io::AsyncWrite
//! [`AsyncBufRead`]: crate::io::AsyncBufRead
//! [`std::io`]: std::io
//! [`tokio::net`]: crate::net
//! [TCP]: crate::net::tcp
//! [UDP]: crate::net::udp
//! [UDS]: crate::net::unix
//! [`tokio::fs`]: crate::fs
//! [`std::fs`]: std::fs
//! [`tokio::signal`]: crate::signal
//! [`tokio::process`]: crate::process
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! use tokio::net::TcpListener;
//! use tokio::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
//!
//!     loop {
//!         let (mut socket, _) = listener.accept().await?;
//!
//!         tokio::spawn(async move {
//!             let mut buf = [0; 1024];
//!
//!             // In a loop, read data from the socket and write the data back.
//!             loop {
//!                 let n = match socket.read(&mut buf).await {
//!                     // socket closed
//!                     Ok(n) if n == 0 => return,
//!                     Ok(n) => n,
//!                     Err(e) => {
//!                         eprintln!("failed to read from socket; err = {:?}", e);
//!                         return;
//!                     }
//!                 };
//!
//!                 // Write the data back
//!                 if let Err(e) = socket.write_all(&buf[0..n]).await {
//!                     eprintln!("failed to write to socket; err = {:?}", e);
//!                     return;
//!                 }
//!             }
//!         });
//!     }
//! }
//! ```

// macros used internally
#[macro_use]
mod macros;

cfg_fs! {
    pub mod fs;
}

mod future;

pub mod io;
pub mod net;

mod loom;
mod park;

pub mod prelude;

cfg_process! {
    pub mod process;
}

pub mod runtime;

cfg_signal! {
    pub mod signal;
}

cfg_sync! {
    pub mod sync;
}
cfg_not_sync! {
    mod sync;
}

cfg_rt_core! {
    pub mod task;
    pub use task::spawn;
}

cfg_time! {
    pub mod time;
}

mod util;

cfg_macros! {
    #[cfg(not(test))] // Work around for rust-lang/rust#62127
    pub use tokio_macros::main;
    pub use tokio_macros::test;
}

// Tests
#[cfg(test)]
mod tests;

// TODO: rm
#[cfg(feature = "io-util")]
#[cfg(test)]
fn is_unpin<T: Unpin>() {}
