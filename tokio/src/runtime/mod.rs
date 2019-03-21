//! A batteries included runtime for applications using Tokio.
//!
//! Applications using Tokio require some runtime support in order to work:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
//! * A [timer] for scheduling work to run after a set period of time.
//!
//! While it is possible to setup each component manually, this involves a bunch
//! of boilerplate.
//!
//! [`Runtime`] bundles all of these various runtime components into a single
//! handle that can be started and shutdown together, eliminating the necessary
//! boilerplate to run a Tokio application.
//!
//! Most applications wont need to use [`Runtime`] directly. Instead, they will
//! use the [`run`] function, which uses [`Runtime`] under the hood.
//!
//! Creating a [`Runtime`] does the following:
//!
//! * Spawn a background thread running a [`Reactor`] instance.
//! * Start a [`ThreadPool`] for executing futures.
//! * Run an instance of [`Timer`] **per** thread pool worker thread.
//!
//! The thread pool uses a work-stealing strategy and is configured to start a
//! worker thread for each CPU core available on the system. This tends to be
//! the ideal setup for Tokio applications.
//!
//! A timer per thread pool worker thread is used to minimize the amount of
//! synchronization that is required for working with the timer.
//!
//! # Usage
//!
//! Most applications will use the [`run`] function. This takes a future to
//! "seed" the application, blocking the thread until the runtime becomes
//! [idle].
//!
//! ```rust
//! # extern crate tokio;
//! # extern crate futures;
//! # use futures::{Future, Stream};
//! use tokio::net::TcpListener;
//!
//! # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
//! # unimplemented!();
//! # }
//! # fn dox() {
//! # let addr = "127.0.0.1:8080".parse().unwrap();
//! let listener = TcpListener::bind(&addr).unwrap();
//!
//! let server = listener.incoming()
//!     .map_err(|e| println!("error = {:?}", e))
//!     .for_each(|socket| {
//!         tokio::spawn(process(socket))
//!     });
//!
//! tokio::run(server);
//! # }
//! # pub fn main() {}
//! ```
//!
//! In this function, the `run` function blocks until the runtime becomes idle.
//! See [`shutdown_on_idle`][idle] for more shutdown details.
//!
//! From within the context of the runtime, additional tasks are spawned using
//! the [`tokio::spawn`] function. Futures spawned using this function will be
//! executed on the same thread pool used by the [`Runtime`].
//!
//! A [`Runtime`] instance can also be used directly.
//!
//! ```rust
//! # extern crate tokio;
//! # extern crate futures;
//! # use futures::{Future, Stream};
//! use tokio::runtime::Runtime;
//! use tokio::net::TcpListener;
//!
//! # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
//! # unimplemented!();
//! # }
//! # fn dox() {
//! # let addr = "127.0.0.1:8080".parse().unwrap();
//! let listener = TcpListener::bind(&addr).unwrap();
//!
//! let server = listener.incoming()
//!     .map_err(|e| println!("error = {:?}", e))
//!     .for_each(|socket| {
//!         tokio::spawn(process(socket))
//!     });
//!
//! // Create the runtime
//! let mut rt = Runtime::new().unwrap();
//!
//! // Spawn the server task
//! rt.spawn(server);
//!
//! // Wait until the runtime becomes idle and shut it down.
//! rt.shutdown_on_idle()
//!     .wait().unwrap();
//! # }
//! # pub fn main() {}
//! ```
//!
//! [reactor]: ../reactor/struct.Reactor.html
//! [executor]: https://tokio.rs/docs/getting-started/runtime-model/#executors
//! [timer]: ../timer/index.html
//! [`Runtime`]: struct.Runtime.html
//! [`Reactor`]: ../reactor/struct.Reactor.html
//! [`ThreadPool`]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool/struct.ThreadPool.html
//! [`run`]: fn.run.html
//! [idle]: struct.Runtime.html#method.shutdown_on_idle
//! [`tokio::spawn`]: ../executor/fn.spawn.html
//! [`Timer`]: https://docs.rs/tokio-timer/0.2/tokio_timer/timer/struct.Timer.html

pub mod current_thread;
mod threadpool;

pub use self::threadpool::{
    Builder,
    Runtime,
    Shutdown,
    TaskExecutor,
    run,
};

