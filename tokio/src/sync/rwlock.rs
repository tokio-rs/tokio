use crate::sync::batch_semaphore::{Semaphore, TryAcquireError};
use crate::sync::mutex::TryLockError;
use std::cell::UnsafeCell;
use std::fmt;
use std::marker;
use std::mem;
use std::ops;

#[cfg(not(loom))]
const MAX_READS: usize = 32;

#[cfg(loom)]
const MAX_READS: usize = 10;

/// An asynchronous reader-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`] does not distinguish between readers or writers
/// that acquire the lock, therefore causing any tasks waiting for the lock to
/// become available to yield. An `RwLock` will allow any number of readers to
/// acquire the lock as long as a writer is not holding the lock.
///
/// The priority policy of Tokio's read-write lock is _fair_ (or
/// [_write-preferring_]), in order to ensure that readers cannot starve
/// writers. Fairness is ensured using a first-in, first-out queue for the tasks
/// awaiting the lock; if a task that wishes to acquire the write lock is at the
/// head of the queue, read locks will not be given out until the write lock has
/// been released. This is in contrast to the Rust standard library's
/// `std::sync::RwLock`, where the priority policy is dependent on the
/// operating system's implementation.
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies [`Send`] to be shared across threads. The RAII guards
/// returned from the locking methods implement [`Deref`](trait@std::ops::Deref)
/// (and [`DerefMut`](trait@std::ops::DerefMut)
/// for the `write` methods) to allow access to the content of the lock.
///
/// # Examples
///
/// ```
/// use tokio::sync::RwLock;
///
/// #[tokio::main]
/// async fn main() {
///     let lock = RwLock::new(5);
///
///     // many reader locks can be held at once
///     {
///         let r1 = lock.read().await;
///         let r2 = lock.read().await;
///         assert_eq!(*r1, 5);
///         assert_eq!(*r2, 5);
///     } // read locks are dropped at this point
///
///     // only one write lock may be held, however
///     {
///         let mut w = lock.write().await;
///         *w += 1;
///         assert_eq!(*w, 6);
///     } // write lock is dropped here
/// }
/// ```
///
/// [`Mutex`]: struct@super::Mutex
/// [`RwLock`]: struct@RwLock
/// [`RwLockReadGuard`]: struct@RwLockReadGuard
/// [`RwLockWriteGuard`]: struct@RwLockWriteGuard
/// [`Send`]: trait@std::marker::Send
/// [_write-preferring_]: https://en.wikipedia.org/wiki/Readers%E2%80%93writer_lock#Priority_policies
#[derive(Debug)]
pub struct RwLock<T: ?Sized> {
    //semaphore to coordinate read and write access to T
    s: Semaphore,

    //inner data T
    c: UnsafeCell<T>,
}

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`read`] method on
/// [`RwLock`].
///
/// [`read`]: method@RwLock::read
/// [`RwLock`]: struct@RwLock
pub struct RwLockReadGuard<'a, T: ?Sized> {
    s: &'a Semaphore,
    data: *const T,
    marker: marker::PhantomData<&'a T>,
}

impl<'a, T> RwLockReadGuard<'a, T> {
    /// Make a new `RwLockReadGuard` for a component of the locked data.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already
    /// locked the data.
    ///
    /// This is an associated function that needs to be
    /// used as `RwLockReadGuard::map(...)`. A method would interfere with
    /// methods of the same name on the contents of the locked data.
    ///
    /// This is an asynchronous version of [`RwLockReadGuard::map`] from the
    /// [`parking_lot` crate].
    ///
    /// [`RwLockReadGuard::map`]: https://docs.rs/lock_api/latest/lock_api/struct.RwLockReadGuard.html#method.map
    /// [`parking_lot` crate]: https://crates.io/crates/parking_lot
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::{RwLock, RwLockReadGuard};
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// struct Foo(u32);
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = RwLock::new(Foo(1));
    ///
    /// let guard = lock.read().await;
    /// let guard = RwLockReadGuard::map(guard, |f| &f.0);
    ///
    /// assert_eq!(1, *guard);
    /// # }
    /// ```
    #[inline]
    pub fn map<F, U: ?Sized>(this: Self, f: F) -> RwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        let data = f(&*this) as *const U;
        let s = this.s;
        // NB: Forget to avoid drop impl from being called.
        mem::forget(this);
        RwLockReadGuard {
            s,
            data,
            marker: marker::PhantomData,
        }
    }

    /// Attempts to make a new [`RwLockReadGuard`] for a component of the
    /// locked data. The original guard is returned if the closure returns
    /// `None`.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already
    /// locked the data.
    ///
    /// This is an associated function that needs to be used as
    /// `RwLockReadGuard::try_map(..)`. A method would interfere with methods of the
    /// same name on the contents of the locked data.
    ///
    /// This is an asynchronous version of [`RwLockReadGuard::try_map`] from the
    /// [`parking_lot` crate].
    ///
    /// [`RwLockReadGuard::try_map`]: https://docs.rs/lock_api/latest/lock_api/struct.RwLockReadGuard.html#method.try_map
    /// [`parking_lot` crate]: https://crates.io/crates/parking_lot
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::{RwLock, RwLockReadGuard};
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// struct Foo(u32);
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = RwLock::new(Foo(1));
    ///
    /// let guard = lock.read().await;
    /// let guard = RwLockReadGuard::try_map(guard, |f| Some(&f.0)).expect("should not fail");
    ///
    /// assert_eq!(1, *guard);
    /// # }
    /// ```
    #[inline]
    pub fn try_map<F, U: ?Sized>(this: Self, f: F) -> Result<RwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        let data = match f(&*this) {
            Some(data) => data as *const U,
            None => return Err(this),
        };
        let s = this.s;
        // NB: Forget to avoid drop impl from being called.
        mem::forget(this);
        Ok(RwLockReadGuard {
            s,
            data,
            marker: marker::PhantomData,
        })
    }
}

impl<'a, T: ?Sized> fmt::Debug for RwLockReadGuard<'a, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> fmt::Display for RwLockReadGuard<'a, T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Drop for RwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        self.s.release(1);
    }
}

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`write`] and method
/// on [`RwLock`].
///
/// [`write`]: method@RwLock::write
/// [`RwLock`]: struct@RwLock
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    s: &'a Semaphore,
    data: *mut T,
    marker: marker::PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> RwLockWriteGuard<'a, T> {
    /// Atomically downgrades a write lock into a read lock without allowing
    /// any writers to take exclusive access of the lock in the meantime.
    ///
    /// **Note:** This won't *necessarily* allow any additional readers to acquire
    /// locks, since [`RwLock`] is fair and it is possible that a writer is next
    /// in line.
    ///
    /// Returns an RAII guard which will drop the read access of this rwlock
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
    /// [`RwLock`]: struct@RwLock
    pub fn downgrade(self) -> RwLockReadGuard<'a, T> {
        let RwLockWriteGuard { s, data, .. } = self;

        // Release all but one of the permits held by the write guard
        s.release(MAX_READS - 1);
        // NB: Forget to avoid drop impl from being called.
        mem::forget(self);
        RwLockReadGuard {
            s,
            data,
            marker: marker::PhantomData,
        }
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
        self.s.release(MAX_READS);
    }
}

#[test]
#[cfg(not(loom))]
fn bounds() {
    fn check_send<T: Send>() {}
    fn check_sync<T: Sync>() {}
    fn check_unpin<T: Unpin>() {}
    // This has to take a value, since the async fn's return type is unnameable.
    fn check_send_sync_val<T: Send + Sync>(_t: T) {}

    check_send::<RwLock<u32>>();
    check_sync::<RwLock<u32>>();
    check_unpin::<RwLock<u32>>();

    check_send::<RwLockReadGuard<'_, u32>>();
    check_sync::<RwLockReadGuard<'_, u32>>();
    check_unpin::<RwLockReadGuard<'_, u32>>();

    check_send::<RwLockWriteGuard<'_, u32>>();
    check_sync::<RwLockWriteGuard<'_, u32>>();
    check_unpin::<RwLockWriteGuard<'_, u32>>();

    let rwlock = RwLock::new(0);
    check_send_sync_val(rwlock.read());
    check_send_sync_val(rwlock.write());
}

// As long as T: Send + Sync, it's fine to send and share RwLock<T> between threads.
// If T were not Send, sending and sharing a RwLock<T> would be bad, since you can access T through
// RwLock<T>.
unsafe impl<T> Send for RwLock<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for RwLock<T> where T: ?Sized + Send + Sync {}
// NB: These impls need to be explicit since we're storing a raw pointer.
// Safety: Stores a raw pointer to `T`, so if `T` is `Sync`, the lock guard over
// `T` is `Send`.
unsafe impl<T> Send for RwLockReadGuard<'_, T> where T: ?Sized + Sync {}
unsafe impl<T> Sync for RwLockReadGuard<'_, T> where T: ?Sized + Send + Sync {}
unsafe impl<T> Sync for RwLockWriteGuard<'_, T> where T: ?Sized + Send + Sync {}
// Safety: Stores a raw pointer to `T`, so if `T` is `Sync`, the lock guard over
// `T` is `Send` - but since this is also provides mutable access, we need to
// make sure that `T` is `Send` since its value can be sent across thread
// boundaries.
unsafe impl<T> Send for RwLockWriteGuard<'_, T> where T: ?Sized + Send + Sync {}

impl<T: ?Sized> RwLock<T> {
    /// Creates a new instance of an `RwLock<T>` which is unlocked.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// let lock = RwLock::new(5);
    /// ```
    pub fn new(value: T) -> RwLock<T>
    where
        T: Sized,
    {
        RwLock {
            c: UnsafeCell::new(value),
            s: Semaphore::new(MAX_READS),
        }
    }

    /// Creates a new instance of an `RwLock<T>` which is unlocked.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// static LOCK: RwLock<i32> = RwLock::const_new(5);
    /// ```
    #[cfg(all(feature = "parking_lot", not(all(loom, test))))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    pub const fn const_new(value: T) -> RwLock<T>
    where
        T: Sized,
    {
        RwLock {
            c: UnsafeCell::new(value),
            s: Semaphore::const_new(MAX_READS),
        }
    }

    /// Locks this rwlock with shared read access, causing the current task
    /// to yield until the lock has been acquired.
    ///
    /// The calling task will yield until there are no more writers which
    /// hold the lock. There may be other readers currently inside the lock when
    /// this method returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let lock = Arc::new(RwLock::new(1));
    ///     let c_lock = lock.clone();
    ///
    ///     let n = lock.read().await;
    ///     assert_eq!(*n, 1);
    ///
    ///     tokio::spawn(async move {
    ///         // While main has an active read lock, we acquire one too.
    ///         let r = c_lock.read().await;
    ///         assert_eq!(*r, 1);
    ///     }).await.expect("The spawned task has panicked");
    ///
    ///     // Drop the guard after the spawned task finishes.
    ///     drop(n);
    ///}
    /// ```
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.s.acquire(1).await.unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        });
        RwLockReadGuard {
            s: &self.s,
            data: self.c.get(),
            marker: marker::PhantomData,
        }
    }

    /// Attempts to acquire this `RwLock` with shared read access.
    ///
    /// If the access couldn't be acquired immediately, returns [`TryLockError`].
    /// Otherwise, an RAII guard is returned which will release read access
    /// when dropped.
    ///
    /// [`TryLockError`]: TryLockError
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let lock = Arc::new(RwLock::new(1));
    ///     let c_lock = lock.clone();
    ///
    ///     let v = lock.try_read().unwrap();
    ///     assert_eq!(*v, 1);
    ///
    ///     tokio::spawn(async move {
    ///         // While main has an active read lock, we acquire one too.
    ///         let n = c_lock.read().await;
    ///         assert_eq!(*n, 1);
    ///     }).await.expect("The spawned task has panicked");
    ///
    ///     // Drop the guard when spawned task finishes.
    ///     drop(v);
    /// }
    /// ```
    pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, TryLockError> {
        match self.s.try_acquire(1) {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => return Err(TryLockError(())),
            Err(TryAcquireError::Closed) => unreachable!(),
        }

        Ok(RwLockReadGuard {
            s: &self.s,
            data: self.c.get(),
            marker: marker::PhantomData,
        })
    }

    /// Locks this rwlock with exclusive write access, causing the current task
    /// to yield until the lock has been acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///   let lock = RwLock::new(1);
    ///
    ///   let mut n = lock.write().await;
    ///   *n = 2;
    ///}
    /// ```
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.s.acquire(MAX_READS as u32).await.unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        });
        RwLockWriteGuard {
            s: &self.s,
            data: self.c.get(),
            marker: marker::PhantomData,
        }
    }

    /// Attempts to acquire this `RwLock` with exclusive write access.
    ///
    /// If the access couldn't be acquired immediately, returns [`TryLockError`].
    /// Otherwise, an RAII guard is returned which will release write access
    /// when dropped.
    ///
    /// [`TryLockError`]: TryLockError
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let rw = RwLock::new(1);
    ///
    ///     let v = rw.read().await;
    ///     assert_eq!(*v, 1);
    ///
    ///     assert!(rw.try_write().is_err());
    /// }
    /// ```
    pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, TryLockError> {
        match self.s.try_acquire(MAX_READS as u32) {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => return Err(TryLockError(())),
            Err(TryAcquireError::Closed) => unreachable!(),
        }

        Ok(RwLockWriteGuard {
            s: &self.s,
            data: self.c.get(),
            marker: marker::PhantomData,
        })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `RwLock` mutably, no actual locking needs to
    /// take place -- the mutable borrow statically guarantees no locks exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// fn main() {
    ///     let mut lock = RwLock::new(1);
    ///
    ///     let n = lock.get_mut();
    ///     *n = 2;
    /// }
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        unsafe {
            // Safety: This is https://github.com/rust-lang/rust/pull/76936
            &mut *self.c.get()
        }
    }

    /// Consumes the lock, returning the underlying data.
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.c.into_inner()
    }
}

impl<T: ?Sized> ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.data }
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

impl<T> From<T> for RwLock<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

impl<T: ?Sized> Default for RwLock<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}
