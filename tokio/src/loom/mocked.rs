pub(crate) use loom::*;

pub(crate) mod sync {
    use std::ops::{Deref, DerefMut};

    pub(crate) use loom::sync::{LockResult, MutexGuard, PoisonError};

    #[derive(Debug)]
    pub(crate) struct Mutex<T>(loom::sync::Mutex<T>);

    #[derive(Debug)]
    pub(crate) struct RwLock<T>(loom::sync::RwLock<T>);

    #[derive(Debug)]
    pub(crate) struct RwLockReadGuard<'a, T>(loom::sync::RwLockReadGuard<'a, T>);

    #[derive(Debug)]
    pub(crate) struct RwLockWriteGuard<'a, T>(loom::sync::RwLockWriteGuard<'a, T>);

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

    #[allow(dead_code)]
    impl<T> RwLock<T> {
        #[inline]
        pub(crate) fn new(t: T) -> RwLock<T> {
            RwLock(loom::sync::RwLock::new(t))
        }

        #[inline]
        pub(crate) fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
            match self.0.read() {
                Ok(inner) => Ok(RwLockReadGuard(inner)),
                Err(err) => Err(PoisonError::new(RwLockReadGuard(err.into_inner()))),
            }
        }

        #[inline]
        pub(crate) fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
            match self.0.write() {
                Ok(inner) => Ok(RwLockWriteGuard(inner)),
                Err(err) => Err(PoisonError::new(RwLockWriteGuard(err.into_inner()))),
            }
        }
    }

    impl<'a, T> Deref for RwLockReadGuard<'a, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0.deref()
        }
    }

    #[allow(dead_code)]
    impl<'a, T> RwLockWriteGuard<'a, T> {
        pub(crate) fn downgrade(
            s: Self,
            rwlock: &'a RwLock<T>,
        ) -> LockResult<RwLockReadGuard<'a, T>> {
            // Std rwlock does not support downgrading.
            drop(s);
            rwlock.read()
        }
    }

    impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0.deref()
        }
    }

    impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut T {
            self.0.deref_mut()
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
