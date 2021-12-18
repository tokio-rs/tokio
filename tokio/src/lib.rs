#![allow(
    clippy::cognitive_complexity,
    clippy::large_enum_variant,
    clippy::needless_doctest_main
)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(unused_must_use)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! A runtime for writing reliable network applications without compromising speed.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * Tools for [working with asynchronous tasks][tasks], including
//!   [synchronization primitives and channels][sync] and [timeouts, sleeps, and
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
//! [website]: https://tokio.rs/tokio/tutorial
//!
//! # A Tour of Tokio
//!
//! Tokio consists of a number of modules that provide a range of functionality
//! essential for implementing asynchronous applications in Rust. In this
//! section, we will take a brief tour of Tokio, summarizing the major APIs and
//! their uses.
//!
//! The easiest way to get started is to enable all features. Do this by
//! enabling the `full` feature flag:
//!
//! ```toml
//! tokio = { version = "1", features = ["full"] }
//! ```
//!
//! ### Authoring applications
//!
//! Tokio is great for writing applications and most users in this case shouldn't
//! worry too much about what features they should pick. If you're unsure, we suggest
//! going with `full` to ensure that you don't run into any road blocks while you're
//! building your application.
//!
//! #### Example
//!
//! This example shows the quickest way to get started with Tokio.
//!
//! ```toml
//! tokio = { version = "1", features = ["full"] }
//! ```
//!
//! ### Authoring libraries
//!
//! As a library author your goal should be to provide the lightest weight crate
//! that is based on Tokio. To achieve this you should ensure that you only enable
//! the features you need. This allows users to pick up your crate without having
//! to enable unnecessary features.
//!
//! #### Example
//!
//! This example shows how you may want to import features for a library that just
//! needs to `tokio::spawn` and use a `TcpStream`.
//!
//! ```toml
//! tokio = { version = "1", features = ["rt", "net"] }
//! ```
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
//! The [`tokio::task`] module is present only when the "rt" feature flag
//! is enabled.
//!
//! [tasks]: task/index.html#what-are-tasks
//! [`tokio::task`]: crate::task
//! [`spawn`]: crate::task::spawn()
//! [`JoinHandle`]: crate::task::JoinHandle
//! [blocking]: task/index.html#blocking-and-yielding
//!
//! The [`tokio::sync`] module contains synchronization primitives to use when
//! needing to communicate or share data. These include:
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
//! tasks, [sleeping][sleep] work to run in the future, or [repeating an operation at an
//! interval][interval].
//!
//! In order to use `tokio::time`, the "time" feature flag must be enabled.
//!
//! [`tokio::time`]: crate::time
//! [sleep]: crate::time::sleep()
//! [interval]: crate::time::interval()
//! [timeout]: crate::time::timeout()
//!
//! Finally, Tokio provides a _runtime_ for executing asynchronous tasks. Most
//! applications can use the [`#[tokio::main]`][main] macro to run their code on the
//! Tokio runtime. However, this macro provides only basic configuration options. As
//! an alternative, the [`tokio::runtime`] module provides more powerful APIs for configuring
//! and managing runtimes. You should use that module if the `#[tokio::main]` macro doesn't
//! provide the functionality you need.
//!
//! Using the runtime requires the "rt" or "rt-multi-thread" feature flags, to
//! enable the basic [single-threaded scheduler][rt] and the [thread-pool
//! scheduler][rt-multi-thread], respectively. See the [`runtime` module
//! documentation][rt-features] for details. In addition, the "macros" feature
//! flag enables the `#[tokio::main]` and `#[tokio::test]` attributes.
//!
//! [main]: attr.main.html
//! [`tokio::runtime`]: crate::runtime
//! [`Builder`]: crate::runtime::Builder
//! [`Runtime`]: crate::runtime::Runtime
//! [rt]: runtime/index.html#current-thread-scheduler
//! [rt-multi-thread]: runtime/index.html#multi-thread-scheduler
//! [rt-features]: runtime/index.html#runtime-scheduler
//!
//! ## CPU-bound tasks and blocking code
//!
//! Tokio is able to concurrently run many tasks on a few threads by repeatedly
//! swapping the currently running task on each thread. However, this kind of
//! swapping can only happen at `.await` points, so code that spends a long time
//! without reaching an `.await` will prevent other tasks from running. To
//! combat this, Tokio provides two kinds of threads: Core threads and blocking
//! threads. The core threads are where all asynchronous code runs, and Tokio
//! will by default spawn one for each CPU core. The blocking threads are
//! spawned on demand, can be used to run blocking code that would otherwise
//! block other tasks from running and are kept alive when not used for a certain
//! amount of time which can be configured with [`thread_keep_alive`].
//! Since it is not possible for Tokio to swap out blocking tasks, like it
//! can do with asynchronous code, the upper limit on the number of blocking
//! threads is very large. These limits can be configured on the [`Builder`].
//!
//! To spawn a blocking task, you should use the [`spawn_blocking`] function.
//!
//! [`Builder`]: crate::runtime::Builder
//! [`spawn_blocking`]: crate::task::spawn_blocking()
//! [`thread_keep_alive`]: crate::runtime::Builder::thread_keep_alive()
//!
//! ```
//! #[tokio::main]
//! async fn main() {
//!     // This is running on a core thread.
//!
//!     let blocking_task = tokio::task::spawn_blocking(|| {
//!         // This is running on a blocking thread.
//!         // Blocking here is ok.
//!     });
//!
//!     // We can wait for the blocking task like this:
//!     // If the blocking task panics, the unwrap below will propagate the
//!     // panic.
//!     blocking_task.await.unwrap();
//! }
//! ```
//!
//! If your code is CPU-bound and you wish to limit the number of threads used
//! to run it, you should use a separate thread pool dedicated to CPU bound tasks.
//! For example, you could consider using the [rayon] library for CPU-bound
//! tasks. It is also possible to create an extra Tokio runtime dedicated to
//! CPU-bound tasks, but if you do this, you should be careful that the extra
//! runtime runs _only_ CPU-bound tasks, as IO-bound tasks on that runtime
//! will behave poorly.
//!
//! Hint: If using rayon, you can use a [`oneshot`] channel to send the result back
//! to Tokio when the rayon task finishes.
//!
//! [rayon]: https://docs.rs/rayon
//! [`oneshot`]: crate::sync::oneshot
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
//! counterpart to [`std::io`].
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
//! [UDP]: crate::net::UdpSocket
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
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let listener = TcpListener::bind("127.0.0.1:8080").await?;
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
//!
//! ## Feature flags
//!
//! Tokio uses a set of [feature flags] to reduce the amount of compiled code. It
//! is possible to just enable certain features over others. By default, Tokio
//! does not enable any features but allows one to enable a subset for their use
//! case. Below is a list of the available feature flags. You may also notice
//! above each function, struct and trait there is listed one or more feature flags
//! that are required for that item to be used. If you are new to Tokio it is
//! recommended that you use the `full` feature flag which will enable all public APIs.
//! Beware though that this will pull in many extra dependencies that you may not
//! need.
//!
//! - `full`: Enables all features listed below except `test-util` and `tracing`.
//! - `rt`: Enables `tokio::spawn`, the basic (current thread) scheduler,
//!         and non-scheduler utilities.
//! - `rt-multi-thread`: Enables the heavier, multi-threaded, work-stealing scheduler.
//! - `io-util`: Enables the IO based `Ext` traits.
//! - `io-std`: Enable `Stdout`, `Stdin` and `Stderr` types.
//! - `net`: Enables `tokio::net` types such as `TcpStream`, `UnixStream` and
//!          `UdpSocket`, as well as (on Unix-like systems) `AsyncFd` and (on
//!          FreeBSD) `PollAio`.
//! - `time`: Enables `tokio::time` types and allows the schedulers to enable
//!           the built in timer.
//! - `process`: Enables `tokio::process` types.
//! - `macros`: Enables `#[tokio::main]` and `#[tokio::test]` macros.
//! - `sync`: Enables all `tokio::sync` types.
//! - `signal`: Enables all `tokio::signal` types.
//! - `fs`: Enables `tokio::fs` types.
//! - `test-util`: Enables testing based infrastructure for the Tokio runtime.
//!
//! _Note: `AsyncRead` and `AsyncWrite` traits do not require any features and are
//! always available._
//!
//! ### Internal features
//!
//! These features do not expose any new API, but influence internal
//! implementation aspects of Tokio, and can pull in additional
//! dependencies.
//!
//! - `parking_lot`: As a potential optimization, use the _parking_lot_ crate's
//! synchronization primitives internally. MSRV may increase according to the
//! _parking_lot_ release in use.
//!
//! ### Unstable features
//!
//! These feature flags enable **unstable** features. The public API may break in 1.x
//! releases. To enable these features, the `--cfg tokio_unstable` must be passed to
//! `rustc` when compiling. This is easiest done using the `RUSTFLAGS` env variable:
//! `RUSTFLAGS="--cfg tokio_unstable"`.
//!
//! - `tracing`: Enables tracing events.
//! - `stats`: Enables runtime stats collection. ([RFC](https://github.com/tokio-rs/tokio/pull/3845))
//!
//! [feature flags]: https://doc.rust-lang.org/cargo/reference/manifest.html#the-features-section

// Test that pointer width is compatible. This asserts that e.g. usize is at
// least 32 bits, which a lot of components in Tokio currently assumes.
//
// TODO: improve once we have MSRV access to const eval to make more flexible.
#[cfg(not(any(
    target_pointer_width = "32",
    target_pointer_width = "64",
    target_pointer_width = "128"
)))]
compile_error! {
    "Tokio requires the platform pointer width to be 32, 64, or 128 bits"
}

// Includes re-exports used by macros.
//
// This module is not intended to be part of the public API. In general, any
// `doc(hidden)` code is not part of Tokio's public and stable API.
#[macro_use]
#[doc(hidden)]
pub mod macros;

cfg_fs! {
    pub mod fs;
}

mod future;

pub mod io;
pub mod net;

mod loom;
mod park;

cfg_process! {
    pub mod process;
}

#[cfg(any(feature = "net", feature = "fs", feature = "io-std"))]
mod blocking;

cfg_rt! {
    pub mod runtime;
}

pub(crate) mod coop;

cfg_signal! {
    pub mod signal;
}

cfg_signal_internal! {
    #[cfg(not(feature = "signal"))]
    #[allow(dead_code)]
    #[allow(unreachable_pub)]
    pub(crate) mod signal;
}

cfg_sync! {
    pub mod sync;
}
cfg_not_sync! {
    mod sync;
}

pub mod task;
cfg_rt! {
    pub use task::spawn;
}

cfg_time! {
    pub mod time;
}

mod util;

/// Due to the `Stream` trait's inclusion in `std` landing later than Tokio's 1.0
/// release, most of the Tokio stream utilities have been moved into the [`tokio-stream`]
/// crate.
///
/// # Why was `Stream` not included in Tokio 1.0?
///
/// Originally, we had planned to ship Tokio 1.0 with a stable `Stream` type
/// but unfortunately the [RFC] had not been merged in time for `Stream` to
/// reach `std` on a stable compiler in time for the 1.0 release of Tokio. For
/// this reason, the team has decided to move all `Stream` based utilities to
/// the [`tokio-stream`] crate. While this is not ideal, once `Stream` has made
/// it into the standard library and the MSRV period has passed, we will implement
/// stream for our different types.
///
/// While this may seem unfortunate, not all is lost as you can get much of the
/// `Stream` support with `async/await` and `while let` loops. It is also possible
/// to create a `impl Stream` from `async fn` using the [`async-stream`] crate.
///
/// [`tokio-stream`]: https://docs.rs/tokio-stream
/// [`async-stream`]: https://docs.rs/async-stream
/// [RFC]: https://github.com/rust-lang/rfcs/pull/2996
///
/// # Example
///
/// Convert a [`sync::mpsc::Receiver`] to an `impl Stream`.
///
/// ```rust,no_run
/// use tokio::sync::mpsc;
///
/// let (tx, mut rx) = mpsc::channel::<usize>(16);
///
/// let stream = async_stream::stream! {
///     while let Some(item) = rx.recv().await {
///         yield item;
///     }
/// };
/// ```
pub mod stream {}

// local re-exports of platform specific things, allowing for decent
// documentation to be shimmed in on docs.rs

#[cfg(docsrs)]
pub mod doc;

#[cfg(docsrs)]
#[allow(unused)]
pub(crate) use self::doc::os;

#[cfg(not(docsrs))]
#[allow(unused)]
pub(crate) use std::os;

#[cfg(docsrs)]
#[allow(unused)]
pub(crate) use self::doc::winapi;

#[cfg(all(not(docsrs), windows, feature = "net"))]
#[allow(unused)]
pub(crate) use ::winapi;

cfg_macros! {
    /// Implementation detail of the `select!` macro. This macro is **not**
    /// intended to be used as part of the public API and is permitted to
    /// change.
    #[doc(hidden)]
    pub use tokio_macros::select_priv_declare_output_enum;

    /// Implementation detail of the `select!` macro. This macro is **not**
    /// intended to be used as part of the public API and is permitted to
    /// change.
    #[doc(hidden)]
    pub use tokio_macros::select_priv_clean_pattern;

    cfg_rt! {
        #[cfg(feature = "rt-multi-thread")]
        #[cfg(not(test))] // Work around for rust-lang/rust#62127
        #[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
        #[doc(inline)]
        pub use tokio_macros::main;

        #[cfg(feature = "rt-multi-thread")]
        #[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
        #[doc(inline)]
        pub use tokio_macros::test;

        cfg_not_rt_multi_thread! {
            #[cfg(not(test))] // Work around for rust-lang/rust#62127
            #[doc(inline)]
            pub use tokio_macros::main_rt as main;

            #[doc(inline)]
            pub use tokio_macros::test_rt as test;
        }
    }

    // Always fail if rt is not enabled.
    cfg_not_rt! {
        #[cfg(not(test))]
        #[doc(inline)]
        pub use tokio_macros::main_fail as main;

        #[doc(inline)]
        pub use tokio_macros::test_fail as test;
    }
}

// TODO: rm
#[cfg(feature = "io-util")]
#[cfg(test)]
fn is_unpin<T: Unpin>() {}
