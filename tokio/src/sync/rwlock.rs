//! An asynchronous `RwLock`-like type.
//!
//! This module provides [`RwLock`], a type that acts similarly to an asynchronous `RwLock`.
//!
//! This allows you to do something along the lines of:
//!
//! ```rust,block_on
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

use crate::sync::semaphore::{AcquireError, Permit, Semaphore};
use crate::sync::Mutex;
use std::cell::UnsafeCell;
use std::future::Future;
use std::ops;
use std::pin::Pin;
use std::task::{Context, Poll};

const MAX_READS: usize = 32;
/// An asynchronous readers writer lock usefull for protecting shared data
///
/// Each lock has a type parameter (`T`) which represents the data that it is protecting. The data
/// can only be accessed through the RAII guards returned from `read` and `write`, which
/// guarantees that the data is only ever modified when the rwlock is with write lock.
#[derive(Debug)]
pub struct RwLock<T> {
    //semaphore to coordinate read and write access to T
    s: Semaphore,

    //inner data T
    c: UnsafeCell<T>,

    //mutex to coordinate writes
    w: Mutex<()>,
}

/// A shared-access handle to a locked RwLock
#[derive(Debug)]
pub struct RwLockReadGuard<'a, T> {
    permit: Permit,
    lock: &'a RwLock<T>,
}

/// An exclusive handle to a locked RwLock
#[derive(Debug)]
pub struct RwLockWriteGuard<'a, T> {
    permits: Vec<Permit>,
    lock: &'a RwLock<T>,
}

struct RwLockReadFuture<'a, T> {
    lock: &'a RwLock<T>,
    permit: Option<Permit>,
}

struct RwLockWriteFuture<'a, T> {
    lock: &'a RwLock<T>,
    permits: Vec<Permit>,
    polling_permit: Option<Permit>,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can access T through
// Mutex<T>.
unsafe impl<T> Send for RwLock<T> where T: Send {}
unsafe impl<T> Sync for RwLock<T> where T: Send {}
unsafe impl<'a, T> Sync for RwLockReadGuard<'a, T> where T: Send + Sync {}
unsafe impl<'a, T> Sync for RwLockWriteGuard<'a, T> where T: Send + Sync {}

impl<T> RwLock<T> {
    /// Creates a new rwlock in an unlocked state ready for use.
    pub fn new(value: T) -> RwLock<T> {
        RwLock {
            c: UnsafeCell::new(value),
            s: Semaphore::new(MAX_READS),
            w: Mutex::new(()),
        }
    }

    /// Acquire the rwlock nonexclusively, read-only
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadFuture {
            lock: self,
            permit: Some(Permit::new()),
        }
        .await
        .unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        })
    }

    /// Acquire the RwLock exclusively, read-write
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        let _lock = self.w.lock().await;
        RwLockWriteFuture {
            lock: self,
            permits: Vec::new(),
            polling_permit: None,
        }
        .await
        .unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        })
    }
}

impl<'a, T> Future for RwLockReadFuture<'a, T> {
    type Output = Result<RwLockReadGuard<'a, T>, AcquireError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut permit = self.permit.take().unwrap();
        match permit.poll_acquire(cx, &self.lock.s) {
            Poll::Pending => {
                self.permit = Some(permit);
                Poll::Pending
            }
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(RwLockReadGuard {
                lock: self.lock,
                permit,
            })),
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
        }
    }
}

impl<'a, T> Future for RwLockWriteFuture<'a, T> {
    type Output = Result<RwLockWriteGuard<'a, T>, AcquireError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while self.permits.len() < MAX_READS {
            let mut permit = match self.polling_permit.take() {
                Some(permit) => permit,
                None => Permit::new(),
            };

            match permit.poll_acquire(cx, &self.lock.s) {
                Poll::Pending => {
                    self.polling_permit = Some(permit);
                    return Poll::Pending;
                }
                Poll::Ready(Ok(())) => {
                    self.permits.push(permit);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            };
        }
        let mut permits = Vec::new();
        permits.append(&mut self.permits);
        Poll::Ready(Ok(RwLockWriteGuard {
            lock: self.lock,
            permits,
        }))
    }
}

impl<T> Drop for RwLockReadFuture<'_, T> {
    fn drop(&mut self) {
        if let Some(mut permit) = self.permit.take() {
            permit.release(&self.lock.s);
        }
    }
}

impl<T> Drop for RwLockWriteFuture<'_, T> {
    fn drop(&mut self) {
        if let Some(mut permit) = self.polling_permit.take() {
            permit.release(&self.lock.s);
        }

        for p in self.permits.iter_mut() {
            p.release(&self.lock.s);
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
