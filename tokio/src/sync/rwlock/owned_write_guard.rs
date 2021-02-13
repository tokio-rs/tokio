use crate::sync::rwlock::owned_read_guard::OwnedRwLockReadGuard;
use crate::sync::rwlock::RwLock;
use std::fmt;
use std::mem::{self, ManuallyDrop};
use std::ops;
use std::sync::Arc;

/// Owned RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`write_owned`] method
/// on [`RwLock`].
///
/// [`write_owned`]: method@crate::sync::RwLock::write_owned
/// [`RwLock`]: struct@crate::sync::RwLock
pub struct OwnedRwLockWriteGuard<T: ?Sized> {
    // ManuallyDrop allows us to destructure into this field without running the destructor.
    pub(super) lock: ManuallyDrop<Arc<RwLock<T>>>,
    pub(super) data: *mut T,
}

impl<T: ?Sized> OwnedRwLockWriteGuard<T> {
    /// Atomically downgrades a write lock into a read lock without allowing
    /// any writers to take exclusive access of the lock in the meantime.
    ///
    /// **Note:** This won't *necessarily* allow any additional readers to acquire
    /// locks, since [`RwLock`] is fair and it is possible that a writer is next
    /// in line.
    ///
    /// Returns an RAII guard which will drop this read access of the `RwLock`
    /// when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::sync::RwLock;
    /// # use std::sync::Arc;
    /// #
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = Arc::new(RwLock::new(1));
    ///
    /// let n = lock.clone().write_owned().await;
    ///
    /// let cloned_lock = lock.clone();
    /// let handle = tokio::spawn(async move {
    ///     *cloned_lock.write_owned().await = 2;
    /// });
    ///
    /// let n = n.downgrade();
    /// assert_eq!(*n, 1, "downgrade is atomic");
    ///
    /// drop(n);
    /// handle.await.unwrap();
    /// assert_eq!(*lock.read().await, 2, "second writer obtained write lock");
    /// # }
    /// ```
    pub fn downgrade(mut self) -> OwnedRwLockReadGuard<T> {
        let lock = unsafe { ManuallyDrop::take(&mut self.lock) };
        let data = self.data;

        // Release all but one of the permits held by the write guard
        lock.s.release(super::MAX_READS - 1);
        // NB: Forget to avoid drop impl from being called.
        mem::forget(self);
        OwnedRwLockReadGuard { lock, data }
    }
}

impl<T: ?Sized> ops::Deref for OwnedRwLockWriteGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.data }
    }
}

impl<T: ?Sized> ops::DerefMut for OwnedRwLockWriteGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data }
    }
}

impl<T: ?Sized> fmt::Debug for OwnedRwLockWriteGuard<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized> fmt::Display for OwnedRwLockWriteGuard<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Drop for OwnedRwLockWriteGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(super::MAX_READS);
        unsafe { ManuallyDrop::drop(&mut self.lock) };
    }
}
