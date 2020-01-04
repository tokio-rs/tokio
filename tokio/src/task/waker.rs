use crate::task::harness::Harness;
use crate::task::{Header, Schedule};

use std::future::Future;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops;
use std::task::{RawWaker, RawWakerVTable, Waker};

pub(super) struct WakerRef<'a, S: 'static> {
    waker: ManuallyDrop<Waker>,
    _p: PhantomData<(&'a Header, S)>,
}

/// Returns a `WakerRef` which avoids having to pre-emptively increase the
/// refcount if there is no need to do so.
pub(super) fn waker_ref<T, S>(meta: &Header) -> WakerRef<'_, S>
where
    T: Future,
    S: Schedule,
{
    let waker = unsafe { ManuallyDrop::new(Waker::from_raw(raw_waker::<T, S>(meta))) };

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

unsafe fn clone_waker<T, S>(ptr: *const ()) -> RawWaker
where
    T: Future,
    S: Schedule,
{
    let meta = ptr as *const Header;
    (*meta).state.ref_inc();
    raw_waker::<T, S>(meta)
}

unsafe fn drop_waker<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let harness = Harness::<T, S>::from_raw(ptr as *mut _);
    harness.drop_waker();
}

unsafe fn wake_by_val<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let harness = Harness::<T, S>::from_raw(ptr as *mut _);
    harness.wake_by_val();
}

// Wake without consuming the waker
unsafe fn wake_by_ref<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let harness = Harness::<T, S>::from_raw(ptr as *mut _);
    harness.wake_by_ref();
}

fn raw_waker<T, S>(meta: *const Header) -> RawWaker
where
    T: Future,
    S: Schedule,
{
    let ptr = meta as *const ();
    let vtable = &RawWakerVTable::new(
        clone_waker::<T, S>,
        wake_by_val::<T, S>,
        wake_by_ref::<T, S>,
        drop_waker::<T, S>,
    );
    RawWaker::new(ptr, vtable)
}
