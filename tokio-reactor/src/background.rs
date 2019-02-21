use {AtomicTask, Handle, Reactor};

use futures::{task, Async, Future, Poll};

use std::io;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::thread;

/// Handle to the reactor running on a background thread.
///
/// Instances are created by calling [`Reactor::background`].
///
/// [`Reactor::background`]: struct.Reactor.html#method.background
#[derive(Debug)]
pub struct Background {
    /// When `None`, the reactor thread will run until the process terminates.
    inner: Option<Inner>,
}

/// Future that resolves when the reactor thread has shutdown.
#[derive(Debug)]
pub struct Shutdown {
    inner: Inner,
}

/// Actual Background handle.
#[derive(Debug)]
struct Inner {
    /// Handle to the reactor
    handle: Handle,

    /// Shared state between the background handle and the reactor thread.
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// Signal the reactor thread to shutdown.
    shutdown: AtomicUsize,

    /// Task to notify when the reactor thread enters a shutdown state.
    shutdown_task: AtomicTask,
}

/// Notifies the reactor thread to shutdown once the reactor becomes idle.
const SHUTDOWN_IDLE: usize = 1;

/// Notifies the reactor thread to shutdown immediately.
const SHUTDOWN_NOW: usize = 2;

/// The reactor is currently shutdown.
const SHUTDOWN: usize = 3;

// ===== impl Background =====

impl Background {
    /// Launch a reactor in the background and return a handle to the thread.
    pub(crate) fn new(reactor: Reactor) -> io::Result<Background> {
        // Grab a handle to the reactor
        let handle = reactor.handle().clone();

        // Create the state shared between the background handle and the reactor
        // thread.
        let shared = Arc::new(Shared {
            shutdown: AtomicUsize::new(0),
            shutdown_task: AtomicTask::new(),
        });

        // For the reactor thread
        let shared2 = shared.clone();

        // Start the reactor thread
        thread::Builder::new().spawn(move || run(reactor, shared2))?;

        Ok(Background {
            inner: Some(Inner { handle, shared }),
        })
    }

    /// Returns a reference to the reactor handle.
    pub fn handle(&self) -> &Handle {
        &self.inner.as_ref().unwrap().handle
    }

    /// Shutdown the reactor on idle.
    ///
    /// Returns a future that completes once the reactor thread has shutdown.
    pub fn shutdown_on_idle(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        inner.shutdown_on_idle();

        Shutdown { inner }
    }

    /// Shutdown the reactor immediately
    ///
    /// Returns a future that completes once the reactor thread has shutdown.
    pub fn shutdown_now(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        inner.shutdown_now();

        Shutdown { inner }
    }

    /// Run the reactor on its thread until the process terminates.
    pub fn forget(mut self) {
        drop(self.inner.take());
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let inner = match self.inner.take() {
            Some(i) => i,
            None => return,
        };

        inner.shutdown_now();

        let shutdown = Shutdown { inner };
        let _ = shutdown.wait();
    }
}

// ===== impl Shutdown =====

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        let task = task::current();
        self.inner.shared.shutdown_task.register_task(task);

        if !self.inner.is_shutdown() {
            return Ok(Async::NotReady);
        }

        Ok(().into())
    }
}

// ===== impl Inner =====

impl Inner {
    /// Returns true if the reactor thread is shutdown.
    fn is_shutdown(&self) -> bool {
        self.shared.shutdown.load(SeqCst) == SHUTDOWN
    }

    /// Notify the reactor thread to shutdown once the reactor transitions to an
    /// idle state.
    fn shutdown_on_idle(&self) {
        self.shared
            .shutdown
            .compare_and_swap(0, SHUTDOWN_IDLE, SeqCst);
        self.handle.wakeup();
    }

    /// Notify the reactor thread to shutdown immediately.
    fn shutdown_now(&self) {
        let mut curr = self.shared.shutdown.load(SeqCst);

        loop {
            if curr >= SHUTDOWN_NOW {
                return;
            }

            let act = self
                .shared
                .shutdown
                .compare_and_swap(curr, SHUTDOWN_NOW, SeqCst);

            if act == curr {
                self.handle.wakeup();
                return;
            }

            curr = act;
        }
    }
}

// ===== impl Reactor thread =====

fn run(mut reactor: Reactor, shared: Arc<Shared>) {
    debug!("starting background reactor");
    loop {
        let shutdown = shared.shutdown.load(SeqCst);

        if shutdown == SHUTDOWN_NOW {
            debug!("shutting background reactor down NOW");
            break;
        }

        if shutdown == SHUTDOWN_IDLE && reactor.is_idle() {
            debug!("shutting background reactor on idle");
            break;
        }

        reactor.turn(None).unwrap();
    }

    drop(reactor);

    // Transition the state to shutdown
    shared.shutdown.store(SHUTDOWN, SeqCst);

    // Notify any waiters
    shared.shutdown_task.notify();

    debug!("background reactor has shutdown");
}
