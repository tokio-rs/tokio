pub(crate) use loom::*;

pub(crate) mod sync {

    pub(crate) use loom::sync::MutexGuard;

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

        #[inline]
        pub(crate) fn get_mut(&mut self) -> &mut T {
            self.0.get_mut().unwrap()
        }
    }
    pub(crate) use loom::sync::*;

    pub(crate) mod atomic {
        pub(crate) use loom::sync::atomic::*;

        // TODO: implement a loom version
        pub(crate) type StaticAtomicU64 = std::sync::atomic::AtomicU64;
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
