use std::sync::{LockResult, TryLockResult, TryLockError};
use std::time::Duration;

use parking_lot as pl;

/// Adapter for `parking_lot::Mutex` to the `std::sync::Mutex` interface.
#[derive(Debug)]
pub(crate) struct Mutex<T: ?Sized>(pl::Mutex<T>);

impl<T> Mutex<T> {
    #[inline]
    pub(crate) fn new(t: T) -> Mutex<T> {
        Mutex(pl::Mutex::new(t))
    }

    #[inline]
    pub(crate) fn lock(&self) -> LockResult<pl::MutexGuard<'_, T>> {
        Ok(self.0.lock())
    }

    #[inline]
    pub(crate) fn try_lock(&self) -> TryLockResult<pl::MutexGuard<'_, T>> {
        match self.0.try_lock() {
            Some(guard) => Ok(guard),
            None => Err(TryLockError::WouldBlock),
        }
    }

    #[allow(unused)]
    #[inline]
    pub(crate) fn is_poisoned(&self) -> bool {
        false
    }

    #[allow(unused)]
    #[inline]
    pub(crate) fn into_inner(self) -> LockResult<T> {
        Ok(self.0.into_inner())
    }
}

/// Adapter for `parking_lot::Condvar` to the `std::sync::Condvar` interface.
#[derive(Debug)]
pub(crate) struct Condvar(pl::Condvar);

impl Condvar {
    #[inline]
    pub(crate) fn new() -> Condvar {
        Condvar(pl::Condvar::new())
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
    pub(crate) fn wait<'a, T>(&self, mut guard: pl::MutexGuard<'a, T>)
        -> LockResult<pl::MutexGuard<'a, T>>
    {
        self.0.wait(&mut guard);
        Ok(guard)
    }

    #[inline]
    pub(crate) fn wait_timeout<'a, T>(
        &self,
        mut guard: pl::MutexGuard<'a, T>,
        timeout: Duration)
        -> LockResult<(pl::MutexGuard<'a, T>, pl::WaitTimeoutResult)>
    {
        let wtr = self.0.wait_for(&mut guard, timeout);
        Ok((guard, wtr))
    }

    // Note: Additional methods `wait_timeout_ms`, `wait_timeout_until`,
    // `wait_until` can be provided here as needed.
}
