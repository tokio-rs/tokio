//! Tokio runtime

use reactor::{self, Reactor};
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
pub fn run<F: FnOnce() -> R, R>(f: F) -> R {
    let mut runtime = Runtime::new().unwrap();
    let ret = runtime.with_context(f).unwrap();

    runtime.shutdown().wait().unwrap();

    ret
}

/// Start the Tokio runtime and spawn the provided future.
pub fn run_seeded<F>(f: F)
where F: Future<Item = (), Error = ()> + Send + 'static,
{
    let mut runtime = Runtime::new().unwrap();
    runtime.spawn(f);
    runtime.shutdown().wait().unwrap();
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

    /// Run the given closure from within the context of the runtime.
    pub fn with_context<F, R>(&mut self, f: F) -> Result<R, EnterError>
    where F: FnOnce() -> R
    {
        let mut enter = tokio_executor::enter()?;

        let inner = self.inner_mut();
        let spawn = inner.pool.sender_mut();
        let reactor = inner.reactor.handle();

        tokio_executor::with_default(spawn, &mut enter, |enter| {
            reactor::with_default(reactor, enter, |_| {
                Ok(f())
            })
        })
    }

    /// Spawn a future into the Tokio runtime.
    pub fn spawn<F>(&mut self, future: F)
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.inner_mut().pool.sender().spawn(future).unwrap();
    }

    /// Shutdown the runtime
    pub fn shutdown(mut self) -> Shutdown {
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
