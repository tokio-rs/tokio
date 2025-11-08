//! Utility for helping miri understand our exposed pointers.

use std::marker::PhantomData;
#[cfg(miri)]
use {crate::loom::sync::Mutex, std::collections::BTreeMap};

pub(crate) struct PtrExposeDomain<T> {
    _phantom: PhantomData<T>,
}

// SAFETY: Actually using the pointers is unsafe, so it's sound to transfer them across threads.
unsafe impl<T> Sync for PtrExposeDomain<T> {}

impl<T> PtrExposeDomain<T> {
    pub(crate) const fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn expose_provenance(&self, ptr: *const T) -> usize {
        #[cfg(miri)]
        {
            ptr.expose_provenance()
        }

        #[cfg(not(miri))]
        {
            ptr as usize
        }
    }

    #[inline]
    #[allow(clippy::wrong_self_convention)] // mirrors std name
    pub(crate) fn from_exposed_addr(&self, addr: usize) -> *const T {
        #[cfg(miri)]
        {
            std::ptr::with_exposed_provenance(addr)
        }

        #[cfg(not(miri))]
        {
            addr as *const T
        }
    }
}
