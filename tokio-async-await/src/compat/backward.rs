use futures::{Future, Poll};

use std::pin::Pin;
use std::future::{
    Future as StdFuture,
};
use std::ptr::NonNull;
use std::task::{
    LocalWaker,
    Poll as StdPoll,
    UnsafeWake,
    Waker,
};

/// Convert an 0.3 `Future` to an 0.1 `Future`.
#[derive(Debug)]
pub struct Compat<T>(Pin<Box<T>>);

impl<T> Compat<T> {
    /// Create a new `Compat` backed by `future`.
    pub fn new(future: T) -> Compat<T> {
        Compat(Box::pinned(future))
    }
}

/// Convert a value into one that can be used with `await!`.
pub trait IntoAwaitable {
    type Awaitable;

    fn into_awaitable(self) -> Self::Awaitable;
}

impl<T> IntoAwaitable for T
where T: StdFuture,
{
    type Awaitable = Self;

    fn into_awaitable(self) -> Self {
        self
    }
}

impl<T, Item, Error> Future for Compat<T>
where T: StdFuture<Output = Result<Item, Error>>,
{
    type Item = Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Item, Error> {
        use futures::Async::*;

        let local_waker = noop_local_waker();

        let res = self.0.as_mut().poll(&local_waker);

        match res {
            StdPoll::Ready(Ok(val)) => Ok(Ready(val)),
            StdPoll::Ready(Err(err)) => Err(err),
            StdPoll::Pending => Ok(NotReady),
        }
    }
}

// ===== NoopWaker =====

struct NoopWaker;

fn noop_local_waker() -> LocalWaker {
    let w: NonNull<NoopWaker> = NonNull::dangling();
    unsafe { LocalWaker::new(w) }
}

fn noop_waker() -> Waker {
    let w: NonNull<NoopWaker> = NonNull::dangling();
    unsafe { Waker::new(w) }
}

unsafe impl UnsafeWake for NoopWaker {
    unsafe fn clone_raw(&self) -> Waker {
        noop_waker()
    }

    unsafe fn drop_raw(&self) {
    }

    unsafe fn wake(&self) {
        panic!("NoopWake cannot wake");
    }
}
