//! A minimal adaption of the `parking_lot` synchronization primitives to the
//! equivalent `std::sync` types.
//!
//! This can be extended to additional types/methods as required.

use std::sync::LockResult;
use std::time::Duration;

// Types that do not need wrapping
pub(crate) use parking_lot::{MutexGuard, RwLockReadGuard, RwLockWriteGuard, WaitTimeoutResult};

/// Adapter for `parking_lot::Mutex` to the `std::sync::Mutex` interface.
#[derive(Debug)]
pub(crate) struct Mutex<T: ?Sized>(parking_lot::Mutex<T>);

#[derive(Debug)]
pub(crate) struct RwLock<T>(parking_lot::RwLock<T>);

/// Adapter for `parking_lot::Condvar` to the `std::sync::Condvar` interface.
#[derive(Debug)]
pub(crate) struct Condvar(parking_lot::Condvar);

impl<T> Mutex<T> {
    #[inline]
    pub(crate) fn new(t: T) -> Mutex<T> {
        Mutex(parking_lot::Mutex::new(t))
    }

    #[inline]
    #[cfg(all(feature = "parking_lot", not(all(loom, test)),))]
    #[cfg_attr(docsrs, doc(cfg(all(feature = "parking_lot",))))]
    pub(crate) const fn const_new(t: T) -> Mutex<T> {
        Mutex(parking_lot::const_mutex(t))
    }

    #[inline]
    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock()
    }

    #[inline]
    pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.0.try_lock()
    }

    #[inline]
    pub(crate) fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    // Note: Additional methods `is_poisoned` and `into_inner`, can be
    // provided here as needed.
}

impl<T> RwLock<T> {
    pub(crate) fn new(t: T) -> RwLock<T> {
        RwLock(parking_lot::RwLock::new(t))
    }

    pub(crate) fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        Ok(self.0.read())
    }

    pub(crate) fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        Ok(self.0.write())
    }
}

impl Condvar {
    #[inline]
    pub(crate) fn new() -> Condvar {
        Condvar(parking_lot::Condvar::new())
    }

    #[inline]
    pub(crate) fn notify_one(&self) {
        self.0.notify_one();
    }

    #[inline]
    pub(crate) fn notify_all(&self) {
        self.0.notify_all();
    }

    #[inline]
    pub(crate) fn wait<'a, T>(
        &self,
        mut guard: MutexGuard<'a, T>,
    ) -> LockResult<MutexGuard<'a, T>> {
        self.0.wait(&mut guard);
        Ok(guard)
    }

    #[inline]
    pub(crate) fn wait_timeout<'a, T>(
        &self,
        mut guard: MutexGuard<'a, T>,
        timeout: Duration,
    ) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
        let wtr = self.0.wait_for(&mut guard, timeout);
        Ok((guard, wtr))
    }

    // Note: Additional methods `wait_timeout_ms`, `wait_timeout_until`,
    // `wait_until` can be provided here as needed.
}
