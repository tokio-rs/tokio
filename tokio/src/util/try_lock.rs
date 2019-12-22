use crate::loom::sync::atomic::AtomicBool;

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering::SeqCst;

pub(crate) struct TryLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

pub(crate) struct LockGuard<'a, T> {
    lock: &'a TryLock<T>,
    _p: PhantomData<std::rc::Rc<()>>,
}

unsafe impl<T: Send> Send for TryLock<T> {}
unsafe impl<T: Send> Sync for TryLock<T> {}

unsafe impl<T: Sync> Sync for LockGuard<'_, T> {}

impl<T> TryLock<T> {
    /// Create a new `TryLock`
    pub(crate) fn new(data: T) -> TryLock<T> {
        TryLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Attempt to acquire lock
    pub(crate) fn try_lock(&self) -> Option<LockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_err()
        {
            return None;
        }

        Some(LockGuard {
            lock: self,
            _p: PhantomData,
        })
    }
}

impl<T> Deref for LockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for LockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for LockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, SeqCst);
    }
}
