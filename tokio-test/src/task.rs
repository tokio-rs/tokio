//! Futures task based helpers

#![allow(clippy::mutex_atomic)]

use std::future::Future;
use std::mem;
use std::ops;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use tokio_stream::Stream;

/// TODO: dox
pub fn spawn<T>(task: T) -> Spawn<T> {
    Spawn {
        task: MockTask::new(),
        future: Box::pin(task),
    }
}

/// Future spawned on a mock task
#[derive(Debug)]
pub struct Spawn<T> {
    task: MockTask,
    future: Pin<Box<T>>,
}

/// Mock task
///
/// A mock task is able to intercept and track wake notifications.
#[derive(Debug, Clone)]
struct MockTask {
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

impl<T> Spawn<T> {
    /// Consumes `self` returning the inner value
    pub fn into_inner(self) -> T
    where
        T: Unpin,
    {
        *Pin::into_inner(self.future)
    }

    /// Returns `true` if the inner future has received a wake notification
    /// since the last call to `enter`.
    pub fn is_woken(&self) -> bool {
        self.task.is_woken()
    }

    /// Returns the number of references to the task waker
    ///
    /// The task itself holds a reference. The return value will never be zero.
    pub fn waker_ref_count(&self) -> usize {
        self.task.waker_ref_count()
    }

    /// Enter the task context
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>, Pin<&mut T>) -> R,
    {
        let fut = self.future.as_mut();
        self.task.enter(|cx| f(cx, fut))
    }
}

impl<T: Unpin> ops::Deref for Spawn<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.future
    }
}

impl<T: Unpin> ops::DerefMut for Spawn<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.future
    }
}

impl<T: Future> Spawn<T> {
    /// Polls a future
    pub fn poll(&mut self) -> Poll<T::Output> {
        let fut = self.future.as_mut();
        self.task.enter(|cx| fut.poll(cx))
    }
}

impl<T: Stream> Spawn<T> {
    /// Polls a stream
    pub fn poll_next(&mut self) -> Poll<Option<T::Item>> {
        let stream = self.future.as_mut();
        self.task.enter(|cx| stream.poll_next(cx))
    }
}

impl<T: Future> Future for Spawn<T> {
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}

impl<T: Stream> Stream for Spawn<T> {
    type Item = T::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.future.as_mut().poll_next(cx)
    }
}

impl MockTask {
    /// Creates new mock task
    fn new() -> Self {
        MockTask {
            waker: Arc::new(ThreadWaker::new()),
        }
    }

    /// Runs a closure from the context of the task.
    ///
    /// Any wake notifications resulting from the execution of the closure are
    /// tracked.
    fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>) -> R,
    {
        self.waker.clear();
        let waker = self.waker();
        let mut cx = Context::from_waker(&waker);

        f(&mut cx)
    }

    /// Returns `true` if the inner future has received a wake notification
    /// since the last call to `enter`.
    fn is_woken(&self) -> bool {
        self.waker.is_woken()
    }

    /// Returns the number of references to the task waker
    ///
    /// The task itself holds a reference. The return value will never be zero.
    fn waker_ref_count(&self) -> usize {
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
        // First, try transitioning from IDLE -> NOTIFY, this does not require a lock.
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

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker);

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

unsafe fn drop_waker(raw: *const ()) {
    let _ = from_raw(raw);
}
