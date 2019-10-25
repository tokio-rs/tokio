//! This module abstracts over `loom` and `std::sync` depending on whether we
//! are running tests or not.
pub(crate) use self::inner::*;

#[cfg(all(test, loom))]
mod inner {
    pub(crate) use loom::sync::CausalCell;
    pub(crate) use loom::sync::Mutex;
    pub(crate) mod atomic {
        pub(crate) use loom::sync::atomic::*;
        pub(crate) use std::sync::atomic::Ordering;
    }
}

#[cfg(not(all(test, loom)))]
mod inner {
    use std::cell::UnsafeCell;
    pub(crate) use std::sync::atomic;
    pub(crate) use std::sync::Mutex;

    #[derive(Debug)]
    pub(crate) struct CausalCell<T>(UnsafeCell<T>);

    impl<T> CausalCell<T> {
        pub(crate) fn new(data: T) -> CausalCell<T> {
            CausalCell(UnsafeCell::new(data))
        }

        #[inline(always)]
        pub(crate) fn with<F, R>(&self, f: F) -> R
        where
            F: FnOnce(*const T) -> R,
        {
            f(self.0.get())
        }

        #[inline(always)]
        pub(crate) fn with_mut<F, R>(&self, f: F) -> R
        where
            F: FnOnce(*mut T) -> R,
        {
            f(self.0.get())
        }
    }
}
