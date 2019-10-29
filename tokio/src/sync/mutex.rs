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
//! [`Mutex`]: struct.Mutex.html
//! [`MutexGuard`]: struct.MutexGuard.html

use crate::sync::semaphore;

use futures_util::future::poll_fn;
use std::cell::UnsafeCell;
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
#[derive(Debug)]
pub struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
    permit: semaphore::Permit,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can access T through
// Mutex<T>.
unsafe impl<T> Send for Mutex<T> where T: Send {}
unsafe impl<T> Sync for Mutex<T> where T: Send {}
unsafe impl<'a, T> Sync for MutexGuard<'a, T> where T: Send + Sync {}

#[test]
#[cfg(not(loom))]
fn bounds() {
    fn check<T: Send>() {}
    check::<MutexGuard<'_, u32>>();
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
        let mut permit = semaphore::Permit::new();
        poll_fn(|cx| permit.poll_acquire(cx, &self.s))
            .await
            .unwrap_or_else(|_| {
                // The semaphore was closed. but, we never explicitly close it, and we have a
                // handle to it through the Arc, which means that this can never happen.
                unreachable!()
            });

        MutexGuard { lock: self, permit }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        if self.permit.is_acquired() {
            self.permit.release(&self.lock.s);
        } else if ::std::thread::panicking() {
            // A guard _should_ always hold its permit, but if the thread is already panicking,
            // we don't want to generate a panic-while-panicing, since that's just unhelpful!
        } else {
            unreachable!("Permit not held when MutexGuard was dropped")
        }
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
        assert!(self.permit.is_acquired());
        unsafe { &*self.lock.c.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        assert!(self.permit.is_acquired());
        unsafe { &mut *self.lock.c.get() }
    }
}

impl<'a, T: fmt::Display> fmt::Display for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
