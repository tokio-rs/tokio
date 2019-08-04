//! Futures task based helpers
//!
//! # Example
//!
//! This example will use the `MockTask` to set the current task on
//! poll.
//!
//! ```
//! # use tokio_test::assert_ready_eq;
//! # use tokio_test::task::MockTask;
//! # use futures::{sync::mpsc, Stream, Sink, Future, Async};
//! let mut task = MockTask::new();
//! let (tx, mut rx) = mpsc::channel(5);
//!
//! tx.send(()).wait();
//!
//! assert_ready_eq!(task.enter(|| rx.poll()), Some(()));
//! ```

use tokio_executor::enter;

use pin_convert::AsPinMut;
use std::future::Future;
use std::mem;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// Mock task
///
/// A mock task is able to intercept and track wake notifications.
#[derive(Debug)]
pub struct MockTask {
    waker: Arc<ThreadWaker>,
}

#[derive(Debug)]
struct ThreadWaker {
    state: Mutex<usize>,
    condvar: Condvar,
}

const IDLE: usize = 0;
const WAKE: usize = 1;
const SLEEP: usize = 2;

impl MockTask {
    /// Create a new mock task
    pub fn new() -> Self {
        MockTask {
            waker: Arc::new(ThreadWaker::new()),
        }
    }

    /// Poll a future
    pub fn poll<T, F>(&mut self, mut fut: T) -> Poll<F::Output>
    where
        T: AsPinMut<F>,
        F: Future,
    {
        self.enter(|cx| fut.as_pin_mut().poll(cx))
    }

    /// Run a closure from the context of the task.
    ///
    /// Any wake notifications resulting from the execution of the closure are
    /// tracked.
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>) -> R,
    {
        let _enter = enter().unwrap();

        self.waker.clear();
        let waker = self.waker();
        let mut cx = Context::from_waker(&waker);

        f(&mut cx)
    }

    /// Returns `true` if the inner future has received a wake notification
    /// since the last call to `enter`.
    pub fn is_woken(&self) -> bool {
        self.waker.is_woken()
    }

    /// Returns the number of references to the task waker
    ///
    /// The task itself holds a reference. The return value will never be zero.
    pub fn waker_ref_count(&self) -> usize {
        Arc::strong_count(&self.waker)
    }

    fn waker(&self) -> Waker {
        unsafe {
            let raw = to_raw(self.waker.clone());
            Waker::from_raw(raw)
        }
    }
}

impl Default for MockTask {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadWaker {
    fn new() -> Self {
        ThreadWaker {
            state: Mutex::new(IDLE),
            condvar: Condvar::new(),
        }
    }

    /// Clears any previously received wakes, avoiding potential spurrious
    /// wake notifications. This should only be called immediately before running the
    /// task.
    fn clear(&self) {
        *self.state.lock().unwrap() = IDLE;
    }

    fn is_woken(&self) -> bool {
        match *self.state.lock().unwrap() {
            IDLE => false,
            WAKE => true,
            _ => unreachable!(),
        }
    }

    fn wake(&self) {
        // First, try transitioning from IDLE -> NOTIFY, this does not require a
        // lock.
        let mut state = self.state.lock().unwrap();
        let prev = *state;

        if prev == WAKE {
            return;
        }

        *state = WAKE;

        if prev == IDLE {
            return;
        }

        // The other half is sleeping, so we wake it up.
        assert_eq!(prev, SLEEP);
        self.condvar.notify_one();
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

unsafe fn to_raw(waker: Arc<ThreadWaker>) -> RawWaker {
    RawWaker::new(Arc::into_raw(waker) as *const (), &VTABLE)
}

unsafe fn from_raw(raw: *const ()) -> Arc<ThreadWaker> {
    Arc::from_raw(raw as *const ThreadWaker)
}

unsafe fn clone(raw: *const ()) -> RawWaker {
    let waker = from_raw(raw);

    // Increment the ref count
    mem::forget(waker.clone());

    to_raw(waker)
}

unsafe fn wake(raw: *const ()) {
    let waker = from_raw(raw);
    waker.wake();
}

unsafe fn wake_by_ref(raw: *const ()) {
    let waker = from_raw(raw);
    waker.wake();

    // We don't actually own a reference to the unparker
    mem::forget(waker);
}

unsafe fn drop(raw: *const ()) {
    let _ = from_raw(raw);
}
