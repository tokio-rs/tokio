#![allow(clippy::redundant_clone)]

use crate::park::Park;
use crate::runtime::enter;
use crate::runtime::time;

use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll::Ready;
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

#[derive(Debug)]
pub(super) struct Shell {
    driver: time::Driver,

    /// TODO: don't store this
    waker: Waker,
}

type Handle = <time::Driver as Park>::Unpark;

impl Shell {
    pub(super) fn new(driver: time::Driver) -> Shell {
        // Make sure we don't mess up types (as we do casts later)
        let unpark: Arc<Handle> = Arc::new(driver.unpark());

        let raw_waker = RawWaker::new(
            Arc::into_raw(unpark) as *const Handle as *const (),
            &RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker),
        );

        let waker = unsafe { Waker::from_raw(raw_waker) };

        Shell { driver, waker }
    }

    pub(super) fn block_on<F>(&mut self, mut f: F) -> F::Output
    where
        F: Future,
    {
        let _e = enter();

        let mut f = unsafe { Pin::new_unchecked(&mut f) };
        let mut cx = Context::from_waker(&self.waker);

        loop {
            if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                return v;
            }

            self.driver.park().unwrap();
        }
    }
}

fn clone_waker(ptr: *const ()) -> RawWaker {
    let w1 = unsafe { ManuallyDrop::new(Arc::from_raw(ptr as *const Handle)) };
    let _w2 = ManuallyDrop::new(w1.clone());

    RawWaker::new(
        ptr,
        &RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker),
    )
}

fn wake(ptr: *const ()) {
    use crate::park::Unpark;
    let unpark = unsafe { Arc::from_raw(ptr as *const Handle) };
    (unpark).unpark()
}

fn wake_by_ref(ptr: *const ()) {
    use crate::park::Unpark;

    let unpark = ptr as *const Handle;
    unsafe { (*unpark).unpark() }
}

fn drop_waker(ptr: *const ()) {
    let _ = unsafe { Arc::from_raw(ptr as *const Handle) };
}
