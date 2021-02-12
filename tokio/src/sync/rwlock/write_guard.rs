use crate::sync::batch_semaphore::Semaphore;
use crate::sync::rwlock::read_guard::RwLockReadGuard;
use crate::sync::rwlock::write_guard_mapped::RwLockMappedWriteGuard;
use std::fmt;
use std::marker;
use std::mem;
use std::ops;

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`write`] and method
/// on [`RwLock`].
///
/// [`write`]: method@crate::sync::RwLock::write
/// [`RwLock`]: struct@crate::sync::RwLock
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    pub(super) s: &'a Semaphore,
    pub(super) data: *mut T,
    pub(super) marker: marker::PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> RwLockWriteGuard<'a, T> {
    /// Make a new [`RwLockMappedWriteGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `RwLockWriteGuard` passed in already
    /// locked the data.
    ///
    /// This is an associated function that needs to be used as
    /// `RwLockWriteGuard::map(..)`. A method would interfere with methods of
    /// the same name on the contents of the locked data.
    ///
    /// This is an asynchronous version of [`RwLockWriteGuard::map`] from the
    /// [`parking_lot` crate].
    ///
    /// [`RwLockMappedWriteGuard`]: struct@crate::sync::RwLockMappedWriteGuard
    /// [`RwLockWriteGuard::map`]: https://docs.rs/lock_api/latest/lock_api/struct.RwLockWriteGuard.html#method.map
    /// [`parking_lot` crate]: https://crates.io/crates/parking_lot
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::{RwLock, RwLockWriteGuard};
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// struct Foo(u32);
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = RwLock::new(Foo(1));
    ///
    /// {
    ///     let mut mapped = RwLockWriteGuard::map(lock.write().await, |f| &mut f.0);
    ///     *mapped = 2;
    /// }
    ///
    /// assert_eq!(Foo(2), *lock.read().await);
    /// # }
    /// ```
    #[inline]
    pub fn map<F, U: ?Sized>(mut this: Self, f: F) -> RwLockMappedWriteGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = f(&mut *this) as *mut U;
        let s = this.s;
        // NB: Forget to avoid drop impl from being called.
        mem::forget(this);
        RwLockMappedWriteGuard {
            s,
            data,
            marker: marker::PhantomData,
        }
    }

    /// Attempts to make  a new [`RwLockMappedWriteGuard`] for a component of
    /// the locked data. The original guard is returned if the closure returns
    /// `None`.
    ///
    /// This operation cannot fail as the `RwLockWriteGuard` passed in already
    /// locked the data.
    ///
    /// This is an associated function that needs to be
    /// used as `RwLockWriteGuard::try_map(...)`. A method would interfere with
    /// methods of the same name on the contents of the locked data.
    ///
    /// This is an asynchronous version of [`RwLockWriteGuard::try_map`] from
    /// the [`parking_lot` crate].
    ///
    /// [`RwLockMappedWriteGuard`]: struct@crate::sync::RwLockMappedWriteGuard
    /// [`RwLockWriteGuard::try_map`]: https://docs.rs/lock_api/latest/lock_api/struct.RwLockWriteGuard.html#method.try_map
    /// [`parking_lot` crate]: https://crates.io/crates/parking_lot
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::{RwLock, RwLockWriteGuard};
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// struct Foo(u32);
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = RwLock::new(Foo(1));
    ///
    /// {
    ///     let guard = lock.write().await;
    ///     let mut guard = RwLockWriteGuard::try_map(guard, |f| Some(&mut f.0)).expect("should not fail");
    ///     *guard = 2;
    /// }
    ///
    /// assert_eq!(Foo(2), *lock.read().await);
    /// # }
    /// ```
    #[inline]
    pub fn try_map<F, U: ?Sized>(
        mut this: Self,
        f: F,
    ) -> Result<RwLockMappedWriteGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let data = match f(&mut *this) {
            Some(data) => data as *mut U,
            None => return Err(this),
        };
        let s = this.s;
        // NB: Forget to avoid drop impl from being called.
        mem::forget(this);
        Ok(RwLockMappedWriteGuard {
            s,
            data,
            marker: marker::PhantomData,
        })
    }

    /// Converts this `RwLockWriteGuard` into an `RwLockMappedWriteGuard`. This
    /// method can be used to store a non-mapped guard in a struct field that
    /// expects a mapped guard.
    ///
    /// This is equivalent to calling `RwLockWriteGuard::map(guard, |me| me)`.
    #[inline]
    pub fn into_mapped(this: Self) -> RwLockMappedWriteGuard<'a, T> {
        RwLockWriteGuard::map(this, |me| me)
    }

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
    /// let n = lock.write().await;
    ///
    /// let cloned_lock = lock.clone();
    /// let handle = tokio::spawn(async move {
    ///     *cloned_lock.write().await = 2;
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
    ///
    /// [`RwLock`]: struct@crate::sync::RwLock
    pub fn downgrade(self) -> RwLockReadGuard<'a, T> {
        let RwLockWriteGuard { s, data, .. } = self;

        // Release all but one of the permits held by the write guard
        s.release(super::MAX_READS - 1);
        // NB: Forget to avoid drop impl from being called.
        mem::forget(self);
        RwLockReadGuard {
            s,
            data,
            marker: marker::PhantomData,
        }
    }
}

impl<T: ?Sized> ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.data }
    }
}

impl<T: ?Sized> ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data }
    }
}

impl<'a, T: ?Sized> fmt::Debug for RwLockWriteGuard<'a, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> fmt::Display for RwLockWriteGuard<'a, T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.s.release(super::MAX_READS);
    }
}
