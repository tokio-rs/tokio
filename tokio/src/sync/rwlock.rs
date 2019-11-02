//! An asynchronous `RwLock`-like type.
//!
//! This module provides [`RwLock`], a type that acts similarly to an asynchronous `RwLock`.
//!
//! This allows you to do something along the lines of:
//!
//! ```rust,no_run
//! use tokio::sync::RwLock;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let data1 = Arc::new(RwLock::new(0));
//!     let data2 = data1.clone();
//!
//!     tokio::spawn(async move {
//!         let mut lock = data2.write().await;
//!         *lock += 1;
//!     });
//!
//!     let mut lock = data1.write().await;
//!     *lock += 1;
//! }
//! ```
//!
//! [`RwLock`]: struct.RwLock.html
//! [`RwLockReadGuard`]: struct.RwLockReadGuard.html
//! [`RwLockWriteGuard`]: struct.RwLockWriteGuard.html

use crate::sync::semaphore;
use crate::sync::Mutex;
use futures_util::future::poll_fn;
use std::cell::UnsafeCell;
use std::ops;

const MAX_READS: usize = 32;
/// An asynchronous readers writer lock usefull for protecting shared data
///
/// Each lock has a type parameter (`T`) which represents the data that it is protecting. The data
/// can only be accessed through the RAII guards returned from `read` and `write`, which
/// guarantees that the data is only ever modified when the rwlock is with write lock.
#[derive(Debug)]
pub struct RwLock<T> {
    //semaphore to coordinate read and write access to T
    s: semaphore::Semaphore,

    //inner data T
    c: UnsafeCell<T>,

    //mutex to coordinate writes
    w: Mutex<()>,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can access T through
// Mutex<T>.
unsafe impl<T> Send for RwLock<T> where T: Send {}
unsafe impl<T> Sync for RwLock<T> where T: Send {}
unsafe impl<'a, T> Sync for RwLockReadGuard<'a, T> where T: Send + Sync {}
unsafe impl<'a, T> Sync for RwLockWriteGuard<'a, T> where T: Send + Sync {}

/// A handle to a shared access to `Rwlock`.
#[derive(Debug)]
pub struct RwLockReadGuard<'a, T> {
    permit: semaphore::Permit,
    lock: &'a RwLock<T>,
}

/// A handle to an exclusive access to `Rwlock`.
#[derive(Debug)]
pub struct RwLockWriteGuard<'a, T> {
    permits: Vec<semaphore::Permit>,
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    /// Creates a new rwlock in an unlocked state ready for use.
    pub fn new(value: T) -> RwLock<T> {
        RwLock {
            c: UnsafeCell::new(value),
            s: semaphore::Semaphore::new(MAX_READS),
            w: Mutex::new(()),
        }
    }

    /// A future that resolves on acquiring shared access to the lock and returns the `RwLockReadGuard`.
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        let mut permit = semaphore::Permit::new();
        poll_fn(|cx| permit.poll_acquire(cx, &self.s))
            .await
            .unwrap_or_else(|_| {
                // The semaphore was closed. but, we never explicitly close it, and we have a
                // handle to it through the Arc, which means that this can never happen.
                unreachable!()
            });
        RwLockReadGuard { lock: self, permit }
    }

    /// A future that resolves on acquiring exclusive access to the the lock and returns the `RwLockWriteGuard`.
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        let _lock = self.w.lock().await;
        let mut permits = vec![];
        for _ in 0..MAX_READS {
            let mut permit = semaphore::Permit::new();
            poll_fn(|cx| permit.poll_acquire(cx, &self.s))
                .await
                .unwrap();
            permits.push(permit);
        }
        RwLockWriteGuard {
            lock: self,
            permits,
        }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.permit.release(&self.lock.s);
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        for p in self.permits.iter_mut() {
            p.release(&self.lock.s);
        }
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
