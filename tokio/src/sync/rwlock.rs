use crate::sync::semaphore::{Permit, Semaphore};
use crate::sync::Mutex;
use std::cell::UnsafeCell;
use std::ops;
use futures_util::future::poll_fn;

const MAX_READS: usize = 32;

/// An asynchronous reader-writer lock
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`] does not distinguish between readers or writers
/// that acquire the lock, therefore blocking any tasks waiting for the lock to
/// become available. An `RwLock` will allow any number of readers to acquire the
/// lock as long as a writer is not holding the lock.
///
/// The priority policy of the lock is dependent on the underlying operating
/// system's implementation, and this type does not guarantee that any
/// particular policy will be used.
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies [`Send`] to be shared across threads. The RAII guards
/// returned from the locking methods implement [`Deref`][] (and [`DerefMut`]
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
/// // many reader locks can be held at once
///     {
///         let r1 = lock.read().await;
///         let r2 = lock.read().await;
///         assert_eq!(*r1, 5);
///         assert_eq!(*r2, 5);
///     } // read locks are dropped at this point
///
/// // only one write lock may be held, however
///     {
///         let mut w = lock.write().await;
///         *w += 1;
///         assert_eq!(*w, 6);
///     } // write lock is dropped here
/// }
/// ```
///
/// [`Mutex`]: struct.Mutex.html
/// [`RwLock`]: struct.RwLock.html
/// [`RwLockReadGuard`]: struct.RwLockReadGuard.html
/// [`RwLockWriteGuard`]: struct.RwLockWriteGuard.html
/// [`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
#[derive(Debug)]
pub struct RwLock<T> {
    //semaphore to coordinate read and write access to T
    s: Semaphore,

    //inner data T
    c: UnsafeCell<T>,

    //mutex to coordinate writes
    w: Mutex<()>,
}

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`read`] method on
/// [`RwLock`].
///
/// [`read`]: struct.RwLock.html#method.read
#[derive(Debug)]
pub struct RwLockReadGuard<'a, T> {
    permit: Permit,
    lock: &'a RwLock<T>,
}

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`write`] and method
/// on [`RwLock`].
///
/// [`write`]: struct.RwLock.html#method.write
/// [`RwLock`]: struct.RwLock.html
#[derive(Debug)]
pub struct RwLockWriteGuard<'a, T> {
    permits: Vec<ReleasingPermit<'a, T>>,
    lock: &'a RwLock<T>,
}

// Wrapper arround Permit that releases on Drop
#[derive(Debug)]
struct ReleasingPermit<'a, T>(Permit, &'a RwLock<T>);

impl<'a, T> Drop for ReleasingPermit<'a, T> {
    fn drop(&mut self) {
        self.0.release(&self.1.s);
    }
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can access T through
// Mutex<T>.
unsafe impl<T> Send for RwLock<T> where T: Send {}
unsafe impl<T> Sync for RwLock<T> where T: Send {}
unsafe impl<'a, T> Sync for RwLockReadGuard<'a, T> where T: Send + Sync {}
unsafe impl<'a, T> Sync for RwLockWriteGuard<'a, T> where T: Send + Sync {}

impl<T> RwLock<T> {
    /// Creates a new instance of an `RwLock<T>` which is unlocked.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::RwLock;
    ///
    /// let lock = RwLock::new(5);
    /// ```
    pub fn new(value: T) -> RwLock<T> {
        RwLock {
            c: UnsafeCell::new(value),
            s: Semaphore::new(MAX_READS),
            w: Mutex::new(()),
        }
    }

    /// Locks this rwlock with shared read access, blocking the current task
    /// until it can be acquired.
    ///
    /// The calling task will be blocked until there are no more writers which
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
    ///         let r = c_lock.read().await;
    ///         assert_eq!(*r, 1);
    ///     })
    ///}
    /// ```
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        let mut permit = ReleasingPermit(Permit::new(), self);
        poll_fn(|cx| permit.0.poll_acquire(cx, &self.s))
            .await
            .unwrap_or_else(|_| {
                // The semaphore was closed. but, we never explicitly close it, and we have a
                // handle to it through the Arc, which means that this can never happen.
                unreachable!()
            });
        RwLockReadGuard { lock: self, permit: std::mem::replace(&mut permit.0, Permit::new()) }
    }

    /// Locks this rwlock with exclusive write access, blocking the current
    /// task until it can be acquired.
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
        let _lock = self.w.lock().await;

        let mut permits = vec![];
        for _ in 0..MAX_READS {
            let mut permit = ReleasingPermit(Permit::new(), self);
            poll_fn(|cx| permit.0.poll_acquire(cx, &self.s))
                .await
                .unwrap();
            permits.push(permit);
        }
        RwLockWriteGuard {
            lock: self,
            permits: permits.split_off(0),
        }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.permit.release(&self.lock.s);
    }
}

impl<T> ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T> ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T> ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.c.get() }
    }
}
