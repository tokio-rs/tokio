use crate::sync::batch_semaphore as semaphore;

use std::cell::UnsafeCell;
use std::error::Error;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// An asynchronous `Mutex`-like type.
///
/// This type acts similarly to an asynchronous [`std::sync::Mutex`], with one
/// major difference: [`lock`] does not block. Another difference is that the
/// lock guard can be held across await points.
///
/// There are some situations where you should prefer the mutex from the
/// standard library. Generally this is the case if:
///
///  1. The lock does not need to be held across await points.
///  2. The duration of any single lock is near-instant.
///
/// On the other hand, the Tokio mutex is for the situation where the lock
/// needs to be held for longer periods of time, or across await points.
///
/// # Examples:
///
/// ```rust,no_run
/// use tokio::sync::Mutex;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let data1 = Arc::new(Mutex::new(0));
///     let data2 = Arc::clone(&data1);
///
///     tokio::spawn(async move {
///         let mut lock = data2.lock().await;
///         *lock += 1;
///     });
///
///     let mut lock = data1.lock().await;
///     *lock += 1;
/// }
/// ```
///
///
/// ```rust,no_run
/// use tokio::sync::Mutex;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let count = Arc::new(Mutex::new(0));
///
///     for _ in 0..5 {
///         let my_count = Arc::clone(&count);
///         tokio::spawn(async move {
///             for _ in 0..10 {
///                 let mut lock = my_count.lock().await;
///                 *lock += 1;
///                 println!("{}", lock);
///             }
///         });
///     }
///
///     loop {
///         if *count.lock().await >= 50 {
///             break;
///         }
///     }
///     println!("Count hit 50.");
/// }
/// ```
/// There are a few things of note here to pay attention to in this example.
/// 1. The mutex is wrapped in an [`Arc`] to allow it to be shared across
///    threads.
/// 2. Each spawned task obtains a lock and releases it on every iteration.
/// 3. Mutation of the data protected by the Mutex is done by de-referencing
///    the obtained lock as seen on lines 12 and 19.
///
/// Tokio's Mutex works in a simple FIFO (first in, first out) style where all
/// calls to [`lock`] complete in the order they were performed. In that way the
/// Mutex is "fair" and predictable in how it distributes the locks to inner
/// data. This is why the output of the program above is an in-order count to
/// 50. Locks are released and reacquired after every iteration, so basically,
/// each thread goes to the back of the line after it increments the value once.
/// Finally, since there is only a single valid lock at any given time, there is
/// no possibility of a race condition when mutating the inner value.
///
/// Note that in contrast to [`std::sync::Mutex`], this implementation does not
/// poison the mutex when a thread holding the [`MutexGuard`] panics. In such a
/// case, the mutex will be unlocked. If the panic is caught, this might leave
/// the data protected by the mutex in an inconsistent state.
///
/// [`Mutex`]: struct@Mutex
/// [`MutexGuard`]: struct@MutexGuard
/// [`Arc`]: struct@std::sync::Arc
/// [`std::sync::Mutex`]: struct@std::sync::Mutex
/// [`Send`]: trait@std::marker::Send
/// [`lock`]: method@Mutex::lock
#[derive(Debug)]
pub struct Mutex<T: ?Sized> {
    s: semaphore::Semaphore,
    c: UnsafeCell<T>,
}

/// A handle to a held `Mutex`.
///
/// As long as you have this guard, you have exclusive access to the underlying
/// `T`. The guard internally borrows the `Mutex`, so the mutex will not be
/// dropped while a guard exists.
///
/// The lock is automatically released whenever the guard is dropped, at which
/// point `lock` will succeed yet again.
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

/// An owned handle to a held `Mutex`.
///
/// This guard is only available from a `Mutex` that is wrapped in an [`Arc`]. It
/// is identical to `MutexGuard`, except that rather than borrowing the `Mutex`,
/// it clones the `Arc`, incrementing the reference count. This means that
/// unlike `MutexGuard`, it will have the `'static` lifetime.
///
/// As long as you have this guard, you have exclusive access to the underlying
/// `T`. The guard internally keeps a reference-couned pointer to the original
/// `Mutex`, so even if the lock goes away, the guard remains valid.
///
/// The lock is automatically released whenever the guard is dropped, at which
/// point `lock` will succeed yet again.
///
/// [`Arc`]: std::sync::Arc
pub struct OwnedMutexGuard<T: ?Sized> {
    lock: Arc<Mutex<T>>,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can
// access T through Mutex<T>.
unsafe impl<T> Send for Mutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for Mutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for MutexGuard<'_, T> where T: ?Sized + Send + Sync {}
unsafe impl<T> Sync for OwnedMutexGuard<T> where T: ?Sized + Send + Sync {}

/// Error returned from the [`Mutex::try_lock`] function.
///
/// A `try_lock` operation can only fail if the mutex is already locked.
///
/// [`Mutex::try_lock`]: Mutex::try_lock
#[derive(Debug)]
pub struct TryLockError(());

impl fmt::Display for TryLockError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "operation would block")
    }
}

impl Error for TryLockError {}

#[test]
#[cfg(not(loom))]
fn bounds() {
    fn check_send<T: Send>() {}
    fn check_unpin<T: Unpin>() {}
    // This has to take a value, since the async fn's return type is unnameable.
    fn check_send_sync_val<T: Send + Sync>(_t: T) {}
    fn check_send_sync<T: Send + Sync>() {}
    fn check_static<T: 'static>() {}
    fn check_static_val<T: 'static>(_t: T) {}

    check_send::<MutexGuard<'_, u32>>();
    check_send::<OwnedMutexGuard<u32>>();
    check_unpin::<Mutex<u32>>();
    check_send_sync::<Mutex<u32>>();
    check_static::<OwnedMutexGuard<u32>>();

    let mutex = Mutex::new(1);
    check_send_sync_val(mutex.lock());
    let arc_mutex = Arc::new(Mutex::new(1));
    check_send_sync_val(arc_mutex.clone().lock_owned());
    check_static_val(arc_mutex.lock_owned());
}

impl<T: ?Sized> Mutex<T> {
    /// Creates a new lock in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    ///
    /// let lock = Mutex::new(5);
    /// ```
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            c: UnsafeCell::new(t),
            s: semaphore::Semaphore::new(1),
        }
    }

    /// Locks this mutex, causing the current task
    /// to yield until the lock has been acquired.
    /// When the lock has been acquired, function returns a [`MutexGuard`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mutex = Mutex::new(1);
    ///
    ///     let mut n = mutex.lock().await;
    ///     *n = 2;
    /// }
    /// ```
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        self.acquire().await;
        MutexGuard { lock: self }
    }

    /// Locks this mutex, causing the current task to yield until the lock has
    /// been acquired. When the lock has been acquired, this returns an
    /// [`OwnedMutexGuard`].
    ///
    /// This method is identical to [`Mutex::lock`], except that the returned
    /// guard references the `Mutex` with an [`Arc`] rather than by borrowing
    /// it. Therefore, the `Mutex` must be wrapped in an `Arc` to call this
    /// method, and the guard will live for the `'static` lifetime, as it keeps
    /// the `Mutex` alive by holding an `Arc`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mutex = Arc::new(Mutex::new(1));
    ///
    ///     let mut n = mutex.clone().lock_owned().await;
    ///     *n = 2;
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    pub async fn lock_owned(self: Arc<Self>) -> OwnedMutexGuard<T> {
        self.acquire().await;
        OwnedMutexGuard { lock: self }
    }

    async fn acquire(&self) {
        self.s.acquire(1).await.unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and
            // we own it exclusively, which means that this can never happen.
            unreachable!()
        });
    }

    /// Attempts to acquire the lock, and returns [`TryLockError`] if the
    /// lock is currently held somewhere else.
    ///
    /// [`TryLockError`]: TryLockError
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    /// # async fn dox() -> Result<(), tokio::sync::TryLockError> {
    ///
    /// let mutex = Mutex::new(1);
    ///
    /// let n = mutex.try_lock()?;
    /// assert_eq!(*n, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
        match self.s.try_acquire(1) {
            Ok(_) => Ok(MutexGuard { lock: self }),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Attempts to acquire the lock, and returns [`TryLockError`] if the lock
    /// is currently held somewhere else.
    ///
    /// This method is identical to [`Mutex::try_lock`], except that the
    /// returned  guard references the `Mutex` with an [`Arc`] rather than by
    /// borrowing it. Therefore, the `Mutex` must be wrapped in an `Arc` to call
    /// this method, and the guard will live for the `'static` lifetime, as it
    /// keeps the `Mutex` alive by holding an `Arc`.
    ///
    /// [`TryLockError`]: TryLockError
    /// [`Arc`]: std::sync::Arc
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    /// use std::sync::Arc;
    /// # async fn dox() -> Result<(), tokio::sync::TryLockError> {
    ///
    /// let mutex = Arc::new(Mutex::new(1));
    ///
    /// let n = mutex.clone().try_lock_owned()?;
    /// assert_eq!(*n, 1);
    /// # Ok(())
    /// # }
    pub fn try_lock_owned(self: Arc<Self>) -> Result<OwnedMutexGuard<T>, TryLockError> {
        match self.s.try_acquire(1) {
            Ok(_) => Ok(OwnedMutexGuard { lock: self }),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Consumes the mutex, returning the underlying data.
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Mutex;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mutex = Mutex::new(1);
    ///
    ///     let n = mutex.into_inner();
    ///     assert_eq!(n, 1);
    /// }
    /// ```
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.c.into_inner()
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

impl<T> Default for Mutex<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

// === impl MutexGuard ===

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.s.release(1)
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

// === impl OwnedMutexGuard ===

impl<T: ?Sized> Drop for OwnedMutexGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(1)
    }
}

impl<T: ?Sized> Deref for OwnedMutexGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for OwnedMutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
