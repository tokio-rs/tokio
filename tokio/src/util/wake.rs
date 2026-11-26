use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::task::{RawWaker, Waker};

/// A `Waker` that is only valid for a given lifetime.
#[derive(Debug)]
pub(crate) struct WakerRef<'a> {
    waker: ManuallyDrop<Waker>,
    _p: PhantomData<&'a ()>,
}

impl<'a> WakerRef<'a> {
    /// Creates a `WakerRef` from a `RawWaker` without taking ownership of the
    /// underlying resource.
    ///
    /// # Safety
    ///
    /// The caller must uphold the contract of [`Waker::from_raw`], except that
    /// the `WakerRef` will not invoke `drop` on the `RawWaker` when it is
    /// dropped, and the pointer inside `waker` only needs to remain valid for
    /// the lifetime `'a`.
    pub(crate) unsafe fn from_raw(waker: RawWaker) -> Self {
        Self {
            waker: ManuallyDrop::new(unsafe { Waker::from_raw(waker) }),
            _p: PhantomData,
        }
    }
}

impl Deref for WakerRef<'_> {
    type Target = Waker;

    fn deref(&self) -> &Waker {
        &self.waker
    }
}
