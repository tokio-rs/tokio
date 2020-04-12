//! An asynchronous `Mutex`-like type.
//!
//! This module provides [`Mutex`], a type that acts similarly to an asynchronous `Mutex`, with one
//! major difference: the [`MutexGuard`] returned by `lock` is not tied to the lifetime of the
//! `Mutex`. This enables you to acquire a lock, and then pass that guard into a future, and then
//! release it at some later point in time.
//!
//! This allows you to do something along the lines of:
//!
//! ```rust,no_run
//! use tokio::sync::Mutex;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let data1 = Arc::new(Mutex::new(0));
//!     let data2 = Arc::clone(&data1);
//!
//!     tokio::spawn(async move {
//!         let mut lock = data2.lock().await;
//!         *lock += 1;
//!     });
//!
//!     let mut lock = data1.lock().await;
//!     *lock += 1;
//! }
//! ```
//!
//! Another example
//! ```rust,no_run
//! #![warn(rust_2018_idioms)]
//!
//! use tokio::sync::Mutex;
//! use std::sync::Arc;
//!
//!
//! #[tokio::main]
//! async fn main() {
//!    let count = Arc::new(Mutex::new(0));
//!
//!    for _ in 0..5 {
//!        let my_count = Arc::clone(&count);
//!        tokio::spawn(async move {
//!            for _ in 0..10 {
//!                let mut lock = my_count.lock().await;
//!                *lock += 1;
//!                println!("{}", lock);
//!            }
//!        });
//!    }
//!
//!    loop {
//!        if *count.lock().await >= 50 {
//!            break;
//!        }
//!    }
//!   println!("Count hit 50.");
//! }
//! ```
//! There are a few things of note here to pay attention to in this example.
//! 1. The mutex is wrapped in an [`std::sync::Arc`] to allow it to be shared across threads.
//! 2. Each spawned task obtains a lock and releases it on every iteration.
//! 3. Mutation of the data the Mutex is protecting is done by de-referencing the the obtained lock
//!    as seen on lines 23 and 30.
//!
//! Tokio's Mutex works in a simple FIFO (first in, first out) style where as requests for a lock are
//! made Tokio will queue them up and provide a lock when it is that requester's turn. In that way
//! the Mutex is "fair" and predictable in how it distributes the locks to inner data. This is why
//! the output of this program is an in-order count to 50. Locks are released and reacquired
//! after every iteration, so basically, each thread goes to the back of the line after it increments
//! the value once. Also, since there is only a single valid lock at any given time there is no
//! possibility of a race condition when mutating the inner value.
//!
//! Note that in contrast to `std::sync::Mutex`, this implementation does not
//! poison the mutex when a thread holding the `MutexGuard` panics. In such a
//! case, the mutex will be unlocked. If the panic is caught, this might leave
//! the data protected by the mutex in an inconsistent state.
//!
//! [`Mutex`]: struct@Mutex
//! [`MutexGuard`]: struct@MutexGuard
use crate::coop::CoopFutureExt;
use crate::sync::batch_semaphore as semaphore;

use std::cell::UnsafeCell;
use std::error::Error;
use std::fmt;
use std::ops::{Deref, DerefMut};

/// An asynchronous mutual exclusion primitive useful for protecting shared data
///
/// Each mutex has a type parameter (`T`) which represents the data that it is protecting. The data
/// can only be accessed through the RAII guards returned from `lock`, which
/// guarantees that the data is only ever accessed when the mutex is locked.
#[derive(Debug)]
pub struct Mutex<T> {
    c: UnsafeCell<T>,
    s: semaphore::Semaphore,
}

/// A handle to a held `Mutex`.
///
/// As long as you have this guard, you have exclusive access to the underlying `T`. The guard
/// internally keeps a reference-couned pointer to the original `Mutex`, so even if the lock goes
/// away, the guard remains valid.
///
/// The lock is automatically released whenever the guard is dropped, at which point `lock`
/// will succeed yet again.
pub struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can access T through
// Mutex<T>.
unsafe impl<T> Send for Mutex<T> where T: Send {}
unsafe impl<T> Sync for Mutex<T> where T: Send {}
unsafe impl<'a, T> Sync for MutexGuard<'a, T> where T: Send + Sync {}

/// Error returned from the [`Mutex::try_lock`] function.
///
/// A `try_lock` operation can only fail if the mutex is already locked.
///
/// [`Mutex::try_lock`]: Mutex::try_lock
#[derive(Debug)]
pub struct TryLockError(());

impl fmt::Display for TryLockError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", "operation would block")
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
    check_send::<MutexGuard<'_, u32>>();
    check_unpin::<Mutex<u32>>();
    check_send_sync::<Mutex<u32>>();

    let mutex = Mutex::new(1);
    check_send_sync_val(mutex.lock());
}

impl<T> Mutex<T> {
    /// Creates a new lock in an unlocked state ready for use.
    pub fn new(t: T) -> Self {
        Self {
            c: UnsafeCell::new(t),
            s: semaphore::Semaphore::new(1),
        }
    }

    /// A future that resolves on acquiring the lock and returns the `MutexGuard`.
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        self.s.acquire(1).cooperate().await.unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        });
        MutexGuard { lock: self }
    }

    /// Tries to acquire the lock
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
        match self.s.try_acquire(1) {
            Ok(_) => Ok(MutexGuard { lock: self }),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Consumes the mutex, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.c.into_inner()
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.s.release(1)
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

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: fmt::Display> fmt::Display for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
