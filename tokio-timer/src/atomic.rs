pub use self::imp::AtomicU64;

#[cfg(target_pointer_width = "64")]
mod imp {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    pub struct AtomicU64 {
        inner: AtomicUsize,
    }

    impl AtomicU64 {
        pub fn new(val: u64) -> AtomicU64 {
            AtomicU64 {
                inner: AtomicUsize::new(val as usize),
            }
        }

        pub fn load(&self, ordering: Ordering) -> u64 {
            self.inner.load(ordering) as u64
        }

        pub fn store(&self, val: u64, ordering: Ordering) {
            self.inner.store(val as usize, ordering)
        }
    }
}

#[cfg(not(target_pointer_width = "64"))]
mod imp {
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
    }
}
