#[cfg(not(all(test, loom)))]
mod imp {
    pub(crate) mod future {
        pub(crate) use crate::sync::task::AtomicWaker;
    }

    pub(crate) mod sync {
        pub(crate) use std::sync::atomic;
        pub(crate) use std::sync::Arc;

        use std::cell::UnsafeCell;

        pub(crate) struct CausalCell<T>(UnsafeCell<T>);

        impl<T> CausalCell<T> {
            pub(crate) fn new(data: T) -> CausalCell<T> {
                CausalCell(UnsafeCell::new(data))
            }

            pub(crate) fn with<F, R>(&self, f: F) -> R
            where
                F: FnOnce(*const T) -> R,
            {
                f(self.0.get())
            }

            pub(crate) fn with_mut<F, R>(&self, f: F) -> R
            where
                F: FnOnce(*mut T) -> R,
            {
                f(self.0.get())
            }
        }
    }

    pub(crate) mod thread {
        pub(crate) fn yield_now() {
            ::std::sync::atomic::spin_loop_hint();
        }
    }
}

#[cfg(all(test, loom))]
mod imp {
    pub(crate) use loom::*;
}

pub(crate) use self::imp::*;
