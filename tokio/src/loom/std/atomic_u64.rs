//! Implementation of an atomic u64 cell. On 64 bit platforms, this is a
//! re-export of `AtomicU64`. On 32 bit platforms, this is implemented using a
//! `Mutex`.

pub(crate) use self::imp::AtomicU64;

// `AtomicU64` can only be used on targets with `target_has_atomic` is 64 or greater.
// Once `cfg_target_has_atomic` feature is stable, we can replace it with
// `#[cfg(target_has_atomic = "64")]`.
// Refs: https://github.com/rust-lang/rust/tree/master/src/librustc_target
#[cfg(not(any(target_arch = "arm", target_arch = "mips", target_arch = "powerpc")))]
mod imp {
    pub(crate) use std::sync::atomic::AtomicU64;
}

#[cfg(any(target_arch = "arm", target_arch = "mips", target_arch = "powerpc"))]
mod imp {
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;

    #[derive(Debug)]
    pub(crate) struct AtomicU64 {
        inner: Mutex<u64>,
    }

    impl AtomicU64 {
        pub(crate) fn new(val: u64) -> Self {
            Self {
                inner: Mutex::new(val),
            }
        }

        pub(crate) fn load(&self, _: Ordering) -> u64 {
            *self.inner.lock().unwrap()
        }

        pub(crate) fn store(&self, val: u64, _: Ordering) {
            *self.inner.lock().unwrap() = val;
        }

        pub(crate) fn fetch_or(&self, val: u64, _: Ordering) -> u64 {
            let mut lock = self.inner.lock().unwrap();
            let prev = *lock;
            *lock = prev | val;
            prev
        }

        pub(crate) fn compare_exchange(
            &self,
            current: u64,
            new: u64,
            _success: Ordering,
            _failure: Ordering,
        ) -> Result<u64, u64> {
            let mut lock = self.inner.lock().unwrap();

            if *lock == current {
                *lock = new;
                Ok(current)
            } else {
                Err(*lock)
            }
        }

        pub(crate) fn compare_exchange_weak(
            &self,
            current: u64,
            new: u64,
            success: Ordering,
            failure: Ordering,
        ) -> Result<u64, u64> {
            self.compare_exchange(current, new, success, failure)
        }
    }
}
