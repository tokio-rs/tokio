use crate::task::harness::Harness;
use crate::task::{Header, Schedule};

use std::future::Future;
use std::marker::PhantomData;
use std::ops;
use std::task::{RawWaker, RawWakerVTable, Waker};

pub(super) struct WakerRef<'a, S: 'static> {
    waker: Waker,
    _p: PhantomData<(&'a Header, S)>,
}

/// Returns a `WakerRef` which avoids having to pre-emptively increase the
/// refcount if there is no need to do so.
pub(super) fn waker_ref<T, S, M>(meta: &Header) -> WakerRef<'_, S>
where
    T: Future,
    S: Schedule<M>,
{
    let ptr = meta as *const _ as *const ();

    let vtable = &RawWakerVTable::new(
        clone_waker::<T, S, M>,
        wake_unreachable,
        wake_by_local_ref::<T, S, M>,
        noop,
    );

    let waker = unsafe { Waker::from_raw(RawWaker::new(ptr, vtable)) };

    WakerRef {
        waker,
        _p: PhantomData,
    }
}

impl<S> ops::Deref for WakerRef<'_, S> {
    type Target = Waker;

    fn deref(&self) -> &Waker {
        &self.waker
    }
}

unsafe fn clone_waker<T, S, M>(ptr: *const ()) -> RawWaker
where
    T: Future,
    S: Schedule<M>,
{
    let meta = ptr as *const Header;
    (*meta).state.ref_inc();

    let vtable = &RawWakerVTable::new(
        clone_waker::<T, S, M>,
        wake_by_val::<T, S, M>,
        wake_by_ref::<T, S, M>,
        drop_waker::<T, S, M>,
    );

    RawWaker::new(ptr, vtable)
}

unsafe fn drop_waker<T, S, M>(ptr: *const ())
where
    T: Future,
    S: Schedule<M>,
{
    let harness = Harness::<T, S, M>::from_raw(ptr as *mut _);
    harness.drop_waker();
}

// `wake()` cannot be called on the ref variaant.
unsafe fn wake_unreachable(_data: *const ()) {
    unreachable!();
}

unsafe fn wake_by_val<T, S, M>(ptr: *const ())
where
    T: Future,
    S: Schedule<M>,
{
    let harness = Harness::<T, S, M>::from_raw(ptr as *mut _);
    harness.wake_by_val();
}

// This function can only be called when on the runtime.
unsafe fn wake_by_local_ref<T, S, M>(ptr: *const ())
where
    T: Future,
    S: Schedule<M>,
{
    let harness = Harness::<T, S, M>::from_raw(ptr as *mut _);
    harness.wake_by_local_ref();
}

// Wake without consuming the waker
unsafe fn wake_by_ref<T, S, M>(ptr: *const ())
where
    T: Future,
    S: Schedule<M>,
{
    let harness = Harness::<T, S, M>::from_raw(ptr as *mut _);
    harness.wake_by_ref();
}

unsafe fn noop(_ptr: *const ()) {}
