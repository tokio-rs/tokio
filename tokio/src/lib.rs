#![doc(html_root_url = "https://docs.rs/tokio/0.2.0-alpha.6")]
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

//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * Tools for working with [asynchronous tasks][task], and a multi threaded,
//!   work-stealing based task [scheduler][runtime].
//! * APIs for performing asynchronous IO, including [TCP and UDP][net] sockets,
//!   [filesystem][fs] operations, and [process management][process].
//! * A [driver] for these asynchronous IO operations, backed by the operating
//!   system's event queue (epoll, kqueue, IOCP, etc...).
//! * Utilities for tracking [time], such as setting [timeouts][timeout],
//!   scheduling work to [run in the future][delay] or [repeat at an
//!   interval][interval].
//!
//!
//! Guide level documentation is found on the [website].
//!
//! [driver]: driver/index.html
//! [website]: https://tokio.rs/docs/
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
//!                         println!("failed to read from socket; err = {:?}", e);
//!                         return;
//!                     }
//!                 };
//!
//!                 // Write the data back
//!                 if let Err(e) = socket.write_all(&buf[0..n]).await {
//!                     println!("failed to write to socket; err = {:?}", e);
//!                     return;
//!                 }
//!             }
//!         });
//!     }
//! }
//! ```
//! ## A Tour of Tokio
//!
//! ### Working With Tasks
//!
//! Asynchronous programs in Rust are based around lightweight, non-blocking
//! units of execution called [_tasks_][tasks]. The [`tokio::task`] module provides
//! important tools for working with tasks:
//!
//! * The [`spawn`] function, for scheduling a new task on the Tokio runtime,
//! * A [`JoinHandle`] type, for awaiting the output of a spawned
//!   task,
//! * Functions for [running blocking operations][blocking] in an asynchronous
//!   task context.
//!
//! The `tokio::task` module is present only when the "rt-core" feature flag is
//! enabled.
//!
//! [tasks]: task/index.html#what-are-tasks
//! [`tokio::task`]: crate::task
//! [`spawn`]: crate::task::spawn()
//! [`JoinHandle`]: crate::task::JoinHandle
//! [`blocking`]: task/index.html#blocking-and-yielding
//!
//! The [`tokio::sync`] module contains synchronization primitives to use when
//! need to communicate or share data. These include _channels_, for sending
//! values between tasks; an asynchronous [`Mutex`], for controlling access to a
//! shared, mutable value; and a [`Barrier`] type for multiple tasks to
//! synchronize before beginning a computation.
//!
//! The channels provided by `tokio` include:
//!
//! * , a channel for sending a single value between tasks,
//! * [`mpsc`], a multi-producer, single-consumer channel for multiple values,
//! * [`watch`], a single producer, multi-consumer channel that broadcasts the
//!   most recently sent value.
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
//! scheduling work. It includes the following functions:
//!
//! * [`delay_until`] and [`delay_for`], for waiting until a
//!   deadline is reached or a duration is elapsed, respectively,
//! * [`interval`] and [`interval_at`], to repeat an operation
//!   every time a period of time elapses,
//! * [`timeout`] and [`timeout_at`], to cancel a future if it does not complete
//!   within a given duration.
//!
//! In addition, the [`DelayQueue`] type implements a data structure where items
//! are enqueued with a duration, and yielded when their duration elapses.
//!
//! In order to use `tokio::time`, the "time" feature flag must be enabled.
//!
//! [`tokio::time`]: crate::time
//! [`delay_until`]: crate::time::delay_until()
//! [`delay_for`]: crate::time::delay_for()
//! [`interval`]: crate::time::interval()
//! [`interval_at`]: crate::time::interval_at()
//! [`timeout`]: crate::time::timeout()
//! [`timeout_at`]: crate::time::timeout_at()
//! [`DelayQueue`]: crate::time::DelayQueue
//!
//! Finally, Tokio provides a _runtime_ for executing asynchronous tasks. Most
//! applications can use the [`#[tokio::main]`][main] macro to run their code on the
//! Tokio runtime. In use-cases where more advanced configuration or management
//! of the runtime is required, the [`tokio::runtime`] module includes:
//!
//! * A [`Builder`] for configuring a new runtime,
//! * A [`Runtime`] type that provides a handle to a runtime instance.
//!
//! Using the runtime requires the "rt-core" or "rt-threaded" feature flags, to
//! enable the basic [single-threaded scheduler][rt-core] and the [thread-pool
//! scheduler][rt-threaded], respectively. See the [`runtime` module
//! documentation][rt-features]  for details.  In addition, the "macros" feature
//! flag enables the `#[tokio::main]` and `#[tokio::test]` attributes.
//!
//! [main]: crate::main
//! [`tokio::runtime`]: crate::runtime
//! [`Builder`]: crate::runtime::Builder
//! [`Runtime`]: crate::runtime::Runtime
//! [rt-core]: runtime/index.html#basic-scheduler
//! [rt-threaded]: runtime/index.html#threaded-scheduler
//! [rt-features]: runtime/index.html#runtime-scheduler
//!

// macros used internally
#[macro_use]
mod macros;

// Blocking task implementation
pub(crate) mod blocking;

cfg_fs! {
    pub mod fs;
}

mod future;

pub mod io;

pub mod net;

mod loom;

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
