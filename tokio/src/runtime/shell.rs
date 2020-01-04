#![allow(clippy::redundant_clone)]

use crate::park::Park;
use crate::runtime::enter;

use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll::Ready;
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

#[derive(Debug)]
pub(super) struct Shell<P> {
    driver: P,

    /// TODO: don't store this
    waker: Waker,
}

impl<P> Shell<P>
where
    P: Park,
    P::Error: std::fmt::Debug,
{
    pub(super) fn new(driver: P) -> Shell<P> {
        // Make sure we don't mess up types (as we do casts later)
        let unpark: Arc<P::Unpark> = Arc::new(driver.unpark());

        let raw_waker = RawWaker::new(
            Arc::into_raw(unpark) as *const P::Unpark as *const (),
            &RawWakerVTable::new(
                clone_waker::<P>,
                wake::<P>,
                wake_by_ref::<P>,
                drop_waker::<P>,
            ),
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
            if let Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }

            self.driver.park().unwrap();
        }
    }
}

fn clone_waker<P>(ptr: *const ()) -> RawWaker
where
    P: Park,
{
    let w1 = unsafe { ManuallyDrop::new(Arc::from_raw(ptr as *const P::Unpark)) };
    let _w2 = ManuallyDrop::new(w1.clone());

    RawWaker::new(
        ptr,
        &RawWakerVTable::new(
            clone_waker::<P>,
            wake::<P>,
            wake_by_ref::<P>,
            drop_waker::<P>,
        ),
    )
}

fn wake<P>(ptr: *const ())
where
    P: Park,
{
    use crate::park::Unpark;
    let unpark = unsafe { Arc::from_raw(ptr as *const P::Unpark) };
    (unpark).unpark()
}

fn wake_by_ref<P>(ptr: *const ())
where
    P: Park,
{
    use crate::park::Unpark;

    let unpark = ptr as *const P::Unpark;
    unsafe { (*unpark).unpark() }
}

fn drop_waker<P>(ptr: *const ())
where
    P: Park,
{
    let _ = unsafe { Arc::from_raw(ptr as *const P::Unpark) };
}
