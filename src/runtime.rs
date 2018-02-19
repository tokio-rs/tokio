//! Tokio runtime

use reactor::{self, Reactor, Handle};
use reactor::background::{self, Background};

use tokio_executor;
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

/// Error returned from `with_context`.
#[derive(Debug)]
pub struct EnterError(());

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

// ===== impl EnterError =====

impl From<tokio_executor::EnterError> for EnterError {
    fn from(_: tokio_executor::EnterError) -> Self {
        EnterError(())
    }
}
