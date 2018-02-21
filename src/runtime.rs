//! A batteries included runtime for applications using Tokio.
//!
//! Applications using Tokio require some runtime support in order to work:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
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
//!
//! The thread pool uses a work-stealing strategy and is configured to start a
//! worker thread for each CPU core available on the system. This tends to be
//! the ideal setup for Tokio applications.
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
//! [`Runtime`]: struct.Runtime.html
//! [`ThreadPool`]: ../executor/thread_pool/struct.ThreadPool.html
//! [`run`]: fn.run.html
//! [idle]: struct.Runtime.html#method.idle

use reactor::{self, Reactor, Handle};
use reactor::background::{self, Background};

use tokio_threadpool::{self as threadpool, ThreadPool};
use futures::Poll;
use futures::future::{Future, Join};

use std::io;

/// Handle to the Tokio runtime.
///
/// The Tokio runtime includes a reactor as well as an executor for running
/// tasks.
#[derive(Debug)]
pub struct Runtime {
    inner: Option<Inner>,
}

/// A future that resolves when the Tokio `Runtime` is shut down.
#[derive(Debug)]
pub struct Shutdown {
    inner: Join<background::Shutdown, threadpool::Shutdown>,
}

#[derive(Debug)]
struct Inner {
    /// Reactor running on a background thread.
    reactor: Background,

    /// Task execution pool.
    pool: ThreadPool,
}

// ===== impl Runtime =====

/// Start the Tokio runtime.
pub fn run<F>(f: F)
where F: Future<Item = (), Error = ()> + Send + 'static,
{
    let mut runtime = Runtime::new().unwrap();
    runtime.spawn(f);
    runtime.shutdown_on_idle().wait().unwrap();
}

impl Runtime {
    /// Create a new runtime
    pub fn new() -> io::Result<Self> {
        // Spawn a reactor on a background thread.
        let reactor = Reactor::new()?.background()?;

        // Get a handle to the reactor.
        let handle = reactor.handle().clone();

        let pool = threadpool::Builder::new()
            .around_worker(move |w, enter| {
                reactor::with_default(&handle, enter, |_| {
                    w.run();
                });
            })
            .build();

        Ok(Runtime {
            inner: Some(Inner {
                reactor,
                pool,
            }),
        })
    }

    /// Return a reference to the reactor handle for this runtime instance.
    pub fn handle(&self) -> &Handle {
        self.inner.as_ref().unwrap().reactor.handle()
    }

    /// Spawn a future into the Tokio runtime.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.inner_mut().pool.sender().spawn(future).unwrap();
        self
    }

    /// Shutdown the runtime
    pub fn shutdown_on_idle(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();

        let inner = inner.reactor.shutdown_on_idle()
            .join(inner.pool.shutdown_on_idle());

        Shutdown { inner }
    }

    /// Shutdown the runtime immediately
    pub fn shutdown_now(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();

        let inner = inner.reactor.shutdown_now()
            .join(inner.pool.shutdown_now());

        Shutdown { inner }
    }

    fn inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().unwrap()
    }
}

// ===== impl Shutdown =====

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        try_ready!(self.inner.poll());
        Ok(().into())
    }
}
