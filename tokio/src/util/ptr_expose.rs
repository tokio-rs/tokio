//! Utility for helping miri understand our exposed pointers.
//!
//! During normal execution, this module is equivalent to pointer casts. However, when running
//! under miri, pointer casts are replaced with lookups in a hash map. This makes Tokio compatible
//! with strict provenance when running under miri (which comes with a perf cost).

use std::marker::PhantomData;
#[cfg(miri)]
use {crate::loom::sync::Mutex, std::collections::HashMap};

pub(crate) struct PtrExposeDomain<T> {
    #[cfg(miri)]
    map: Mutex<HashMap<usize, *mut T>>,
    _phantom: PhantomData<T>,
}

impl<T> PtrExposeDomain<T> {
    pub(crate) const fn new() -> Self {
        Self {
            #[cfg(miri)]
            map: Mutex::new(HashMap::new()),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn expose_provenance(&self, ptr: *const T) -> usize {
        #[cfg(miri)]
        {
            // SAFETY: Equivalent to `pointer::addr` which is safe.
            let addr: usize = unsafe { std::mem::transmute(ptr) };
            self.map.lock().insert(addr, ptr);
            addr
        }

        #[cfg(not(miri))]
        {
            ptr as usize
        }
    }

    #[inline]
    pub(crate) fn from_exposed_addr(&self, addr: usize) -> *const T {
        #[cfg(miri)]
        {
            self.map
                .lock()
                .get(&addr)
                .expect("Provided address is not exposed.")
        }

        #[cfg(not(miri))]
        {
            addr as *const T
        }
    }

    #[inline]
    pub(crate) fn unexpose_provenance(&self, _ptr: *const T) {
        #[cfg(miri)]
        {
            // SAFETY: Equivalent to `pointer::addr` which is safe.
            let addr: usize = unsafe { std::mem::transmute(_ptr) };
            self.map
                .lock()
                .remove(addr)
                .expect("Provided address is not exposed.");
        }
    }
}
