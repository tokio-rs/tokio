//! Futures task based helpers
//!
//! # Example
//!
//! This example will use the `MockTask` to set the current task on
//! poll.
//!
//! ```
//! # #[macro_use] extern crate tokio_test;
//! # extern crate futures;
//! # use tokio_test::task::MockTask;
//! # use futures::{sync::mpsc, Stream, Sink, Future, Async};
//! let mut task = MockTask::new();
//! let (tx, mut rx) = mpsc::channel(5);
//!
//! tx.send(()).wait();
//!
//! assert_ready_eq!(task.enter(|| rx.poll()), Some(()));
//! ```

use futures::executor::{spawn, Notify};
use futures::{future, Async};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Mock task
///
/// A mock task is able to intercept and track notifications.
#[derive(Debug)]
pub struct MockTask {
    notify: Arc<ThreadNotify>,
}

#[derive(Debug)]
struct ThreadNotify {
    state: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

const IDLE: usize = 0;
const NOTIFY: usize = 1;
const SLEEP: usize = 2;

impl MockTask {
    /// Create a new mock task
    pub fn new() -> Self {
        MockTask {
            notify: Arc::new(ThreadNotify::new()),
        }
    }

    /// Run a closure from the context of the task.
    ///
    /// Any notifications resulting from the execution of the closure are
    /// tracked.
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.notify.clear();

        let res = spawn(future::lazy(|| Ok::<_, ()>(f()))).poll_future_notify(&self.notify, 0);

        match res.unwrap() {
            Async::Ready(v) => v,
            _ => unreachable!(),
        }
    }

    /// Returns `true` if the inner future has received a readiness notification
    /// since the last call to `enter`.
    pub fn is_notified(&self) -> bool {
        self.notify.is_notified()
    }

    /// Returns the number of references to the task notifier
    ///
    /// The task itself holds a reference. The return value will never be zero.
    pub fn notifier_ref_count(&self) -> usize {
        Arc::strong_count(&self.notify)
    }
}

impl ThreadNotify {
    fn new() -> Self {
        ThreadNotify {
            state: AtomicUsize::new(IDLE),
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }

    /// Clears any previously received notify, avoiding potential spurrious
    /// notifications. This should only be called immediately before running the
    /// task.
    fn clear(&self) {
        self.state.store(IDLE, Ordering::SeqCst);
    }

    fn is_notified(&self) -> bool {
        match self.state.load(Ordering::SeqCst) {
            IDLE => false,
            NOTIFY => true,
            _ => unreachable!(),
        }
    }
}

impl Notify for ThreadNotify {
    fn notify(&self, _unpark_id: usize) {
        // First, try transitioning from IDLE -> NOTIFY, this does not require a
        // lock.
        match self.state.compare_and_swap(IDLE, NOTIFY, Ordering::SeqCst) {
            IDLE | NOTIFY => return,
            SLEEP => {}
            _ => unreachable!(),
        }

        // The other half is sleeping, this requires a lock
        let _m = self.mutex.lock().unwrap();

        // Transition from SLEEP -> NOTIFY
        match self.state.compare_and_swap(SLEEP, NOTIFY, Ordering::SeqCst) {
            SLEEP => {}
            _ => return,
        }

        // Wakeup the sleeper
        self.condvar.notify_one();
    }
}
