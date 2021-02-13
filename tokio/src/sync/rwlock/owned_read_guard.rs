use crate::sync::rwlock::RwLock;
use std::fmt;
use std::ops;
use std::sync::Arc;

/// Owned RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`read_owned`] method on
/// [`RwLock`].
///
/// [`read_owned`]: method@crate::sync::RwLock::read_owned
/// [`RwLock`]: struct@crate::sync::RwLock
pub struct OwnedRwLockReadGuard<T: ?Sized> {
    pub(super) lock: Arc<RwLock<T>>,
    pub(super) data: *const T,
}

impl<T: ?Sized> ops::Deref for OwnedRwLockReadGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.data }
    }
}

impl<T: ?Sized> fmt::Debug for OwnedRwLockReadGuard<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized> fmt::Display for OwnedRwLockReadGuard<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Drop for OwnedRwLockReadGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
    }
}
