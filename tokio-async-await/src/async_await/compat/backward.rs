use futures::{
    Future as Future01,
    Poll as Poll01,
};
use futures_core::{Future as Future03};

use std::pin::PinBox;
use std::future::FutureObj;
use std::ptr::NonNull;
use std::task::{
    Context,
    Spawn,
    UnsafeWake,
    LocalWaker,
    Poll as Poll03,
    Waker,
    SpawnObjError,
};

/// Convert an 0.3 `Future` to an 0.1 `Future`.
#[derive(Debug)]
pub struct Compat<T>(PinBox<T>);

impl<T> Compat<T> {
    pub fn new(data: T) -> Compat<T> {
        Compat(PinBox::new(data))
    }
}

/// Convert a value into one that can be used with `await!`.
pub trait IntoAwaitable {
    type Awaitable;

    fn into_awaitable(self) -> Self::Awaitable;
}

impl<T> IntoAwaitable for T
where T: Future03,
{
    type Awaitable = Self;

    fn into_awaitable(self) -> Self {
        self
    }
}

impl<T, Item, Error> Future01 for Compat<T>
where T: Future03<Output = Result<Item, Error>>,
{
    type Item = Item;
    type Error = Error;

    fn poll(&mut self) -> Poll01<Item, Error> {
        use futures::Async::*;

        let local_waker = noop_local_waker();
        let mut executor = NoopExecutor;

        let mut cx = Context::new(&local_waker, &mut executor);

        let res = self.0.as_pin_mut().poll(&mut cx);

        match res {
            Poll03::Ready(Ok(val)) => Ok(Ready(val)),
            Poll03::Ready(Err(err)) => Err(err),
            Poll03::Pending => Ok(NotReady),
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

// ===== NoopExecutor =====

struct NoopExecutor;

impl Spawn for NoopExecutor {
    fn spawn_obj(&mut self, future: FutureObj<'static, ()>) -> Result<(), SpawnObjError> {
        use std::task::SpawnErrorKind;

        // NoopExecutor cannot execute
        Err(SpawnObjError {
            kind: SpawnErrorKind::shutdown(),
            future,
        })
    }
}
