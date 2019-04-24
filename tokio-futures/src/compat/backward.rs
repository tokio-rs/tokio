//! Converts a `std::future::Future` into an 0.1 `Future.

use futures::{Future, Poll};

use std::future::Future as StdFuture;
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll as StdPoll, RawWaker, RawWakerVTable, Waker};

/// Converts a `std::future::Future` into an 0.1 `Future.
#[derive(Debug)]
pub struct Compat<T>(Pin<Box<T>>);

impl<T> Compat<T> {
    /// Create a new `Compat` backed by `future`.
    pub(crate) fn new(future: T) -> Compat<T> {
        Compat(Box::pin(future))
    }
}

#[doc(hidden)]
pub trait IntoAwaitable {
    type Awaitable;

    fn into_awaitable(self) -> Self::Awaitable;
}

impl<T> IntoAwaitable for T
where
    T: StdFuture,
{
    type Awaitable = Self;

    fn into_awaitable(self) -> Self {
        self
    }
}

impl<T, Item, Error> Future for Compat<T>
where
    T: StdFuture<Output = Result<Item, Error>>,
{
    type Item = Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Item, Error> {
        use futures::Async::*;

        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let res = self.0.as_mut().poll(&mut context);

        match res {
            StdPoll::Ready(Ok(val)) => Ok(Ready(val)),
            StdPoll::Ready(Err(err)) => Err(err),
            StdPoll::Pending => Ok(NotReady),
        }
    }
}

// ===== NoopWaker =====

fn noop_raw_waker() -> RawWaker {
    RawWaker::new(ptr::null(), &NOOP_WAKER_VTABLE)
}

fn noop_waker() -> Waker {
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

unsafe fn clone_raw(_data: *const ()) -> RawWaker {
    noop_raw_waker()
}

unsafe fn drop_raw(_data: *const ()) {}

unsafe fn wake(_data: *const ()) {
    unimplemented!(
        "async-await-preview currently only supports futures 0.1. Use \
         the compatibility layer of futures 0.3 instead, if you want \
         to use futures 0.3."
    );
}

const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(clone_raw, wake, wake, drop_raw);
