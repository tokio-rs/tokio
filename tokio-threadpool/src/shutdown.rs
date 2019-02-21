use task::Task;
use worker;

use crossbeam_deque::Injector;
use futures::task::AtomicTask;
use futures::{Async, Future, Poll};

use std::sync::{Arc, Mutex};

/// Future that resolves when the thread pool is shutdown.
///
/// A `ThreadPool` is shutdown once all the worker have drained their queues and
/// shutdown their threads.
///
/// `Shutdown` is returned by [`shutdown`], [`shutdown_on_idle`], and
/// [`shutdown_now`].
///
/// [`shutdown`]: struct.ThreadPool.html#method.shutdown
/// [`shutdown_on_idle`]: struct.ThreadPool.html#method.shutdown_on_idle
/// [`shutdown_now`]: struct.ThreadPool.html#method.shutdown_now
#[derive(Debug)]
pub struct Shutdown {
    inner: Arc<Mutex<Inner>>,
}

/// Shared state between `Shutdown` and `ShutdownTrigger`.
///
/// This is used for notifying the `Shutdown` future when `ShutdownTrigger` gets dropped.
#[derive(Debug)]
struct Inner {
    /// The task to notify when the threadpool completes the shutdown process.
    task: AtomicTask,
    /// `true` if the threadpool has been shut down.
    completed: bool,
}

impl Shutdown {
    pub(crate) fn new(trigger: &ShutdownTrigger) -> Shutdown {
        Shutdown {
            inner: trigger.inner.clone(),
        }
    }
}

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        let inner = self.inner.lock().unwrap();

        if !inner.completed {
            inner.task.register();
            Ok(Async::NotReady)
        } else {
            Ok(().into())
        }
    }
}

/// When dropped, cleans up threadpool's resources and completes the shutdown process.
#[derive(Debug)]
pub(crate) struct ShutdownTrigger {
    inner: Arc<Mutex<Inner>>,
    workers: Arc<[worker::Entry]>,
    queue: Arc<Injector<Arc<Task>>>,
}

unsafe impl Send for ShutdownTrigger {}
unsafe impl Sync for ShutdownTrigger {}

impl ShutdownTrigger {
    pub(crate) fn new(
        workers: Arc<[worker::Entry]>,
        queue: Arc<Injector<Arc<Task>>>,
    ) -> ShutdownTrigger {
        ShutdownTrigger {
            inner: Arc::new(Mutex::new(Inner {
                task: AtomicTask::new(),
                completed: false,
            })),
            workers,
            queue,
        }
    }
}

impl Drop for ShutdownTrigger {
    fn drop(&mut self) {
        // Drain the global task queue.
        while !self.queue.steal().is_empty() {}

        // Drop the remaining incomplete tasks and parkers assosicated with workers.
        for worker in self.workers.iter() {
            worker.shutdown();
        }

        // Notify the task interested in shutdown.
        let mut inner = self.inner.lock().unwrap();
        inner.completed = true;
        inner.task.notify();
    }
}
