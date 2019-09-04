//! Implementation of an atomic u64 cell. On 64 bit platforms, this is a
//! re-export of `AtomicU64`. On 32 bit platforms, this is implemented using a
//! `Mutex`.

pub(crate) use self::imp::AtomicU64;

#[cfg(target_pointer_width = "64")]
mod imp {
    pub(crate) use std::sync::atomic::AtomicU64;
}

#[cfg(not(target_pointer_width = "64"))]
mod imp {
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;

    #[derive(Debug)]
    pub struct AtomicU64 {
        inner: Mutex<u64>,
    }

    impl AtomicU64 {
        pub fn new(val: u64) -> AtomicU64 {
            AtomicU64 {
                inner: Mutex::new(val),
            }
        }

        pub fn load(&self, _: Ordering) -> u64 {
            *self.inner.lock().unwrap()
        }

        pub fn store(&self, val: u64, _: Ordering) {
            *self.inner.lock().unwrap() = val;
        }

        pub fn fetch_or(&self, val: u64, _: Ordering) -> u64 {
            let mut lock = self.inner.lock().unwrap();
            let prev = *lock;
            *lock = prev | val;
            prev
        }

        pub fn compare_and_swap(&self, old: u64, new: u64, _: Ordering) -> u64 {
            let mut lock = self.inner.lock().unwrap();
            let prev = *lock;

            if prev != old {
                return prev;
            }

            *lock = new;
            prev
        }
    }
}
