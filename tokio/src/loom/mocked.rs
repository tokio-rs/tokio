pub(crate) use loom::*;

pub(crate) mod sync {

    pub(crate) use loom::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};

    #[derive(Debug)]
    pub(crate) struct Mutex<T>(loom::sync::Mutex<T>);

    #[allow(dead_code)]
    impl<T> Mutex<T> {
        #[inline]
        pub(crate) fn new(t: T) -> Mutex<T> {
            Mutex(loom::sync::Mutex::new(t))
        }

        #[inline]
        #[track_caller]
        pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
            self.0.lock().unwrap()
        }

        #[inline]
        pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
            self.0.try_lock().ok()
        }
    }

    #[derive(Debug)]
    pub(crate) struct RwLock<T>(loom::sync::RwLock<T>);

    #[allow(dead_code)]
    impl<T> RwLock<T> {
        #[inline]
        pub(crate) fn new(t: T) -> Self {
            Self(loom::sync::RwLock::new(t))
        }

        #[inline]
        pub(crate) fn read(&self) -> RwLockReadGuard<'_, T> {
            self.0.read().unwrap()
        }

        #[inline]
        pub(crate) fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
            self.0.try_read().ok()
        }

        #[inline]
        pub(crate) fn write(&self) -> RwLockWriteGuard<'_, T> {
            self.0.write().unwrap()
        }

        #[inline]
        pub(crate) fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
            self.0.try_write().ok()
        }
    }

    pub(crate) use loom::sync::*;

    pub(crate) mod atomic {
        pub(crate) use loom::sync::atomic::*;
        use std::sync::OnceLock;

        pub(crate) struct StaticAtomicU64 {
            init: u64,
            cell: OnceLock<crate::loom::sync::Mutex<u64>>,
        }

        impl StaticAtomicU64 {
            pub(crate) const fn new(val: u64) -> StaticAtomicU64 {
                StaticAtomicU64 {
                    init: val,
                    cell: OnceLock::new(),
                }
            }

            /// Loads the atomic value.
            ///
            /// Note: Ordering parameter is ignored; Loom uses Mutex to simulate atomic behavior.
            pub(crate) fn load(&self, _order: Ordering) -> u64 {
                *self.inner().lock()
            }

            /// Atomically adds to the current value, returning the previous value.
            ///
            /// Note: Ordering parameter is ignored; Loom uses Mutex to simulate atomic behavior.
            pub(crate) fn fetch_add(&self, val: u64, _order: Ordering) -> u64 {
                let mut lock = self.inner().lock();
                let prev = *lock;
                *lock = prev + val;
                prev
            }

            /// Atomically compares and exchanges the value.
            ///
            /// Uses a Mutex for synchronization, so never fails spuriously.
            ///
            /// Note: Ordering parameters are ignored; Loom uses Mutex to simulate atomic behavior.
            pub(crate) fn compare_exchange_weak(
                &self,
                current: u64,
                new: u64,
                _success: Ordering,
                _failure: Ordering,
            ) -> Result<u64, u64> {
                let mut lock = self.inner().lock();

                if *lock == current {
                    *lock = new;
                    Ok(current)
                } else {
                    Err(*lock)
                }
            }

            /// Stores a value into the atomic.
            ///
            /// Note: Ordering parameter is ignored; Loom uses Mutex to simulate atomic behavior.
            pub(crate) fn store(&self, val: u64, _order: Ordering) {
                *self.inner().lock() = val;
            }

            /// Atomically compares and exchanges the value.
            ///
            /// Loom's `compare_exchange` just delegates to `compare_exchange_weak`,
            /// since Mutex provides total ordering.
            ///
            /// Note: Ordering parameters are ignored; Loom uses Mutex to simulate atomic behavior.
            pub(crate) fn compare_exchange(
                &self,
                current: u64,
                new: u64,
                success: Ordering,
                failure: Ordering,
            ) -> Result<u64, u64> {
                self.compare_exchange_weak(current, new, success, failure)
            }

            fn inner(&self) -> &crate::loom::sync::Mutex<u64> {
                self.cell
                    .get_or_init(|| crate::loom::sync::Mutex::new(self.init))
            }
        }

        impl std::fmt::Debug for StaticAtomicU64 {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("StaticAtomicU64")
                    .field("value", &self.load(Ordering::Relaxed))
                    .finish()
            }
        }

        impl Default for StaticAtomicU64 {
            fn default() -> Self {
                StaticAtomicU64::new(0)
            }
        }
    }
}

pub(crate) mod rand {
    pub(crate) fn seed() -> u64 {
        1
    }
}

pub(crate) mod sys {
    pub(crate) fn num_cpus() -> usize {
        2
    }
}

pub(crate) mod thread {
    pub use loom::lazy_static::AccessError;
    pub use loom::thread::*;
}
