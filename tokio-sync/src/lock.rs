//! An asynchronous `Mutex`-like type.
//!
//! This module provides [`Lock`], a type that acts similarly to an asynchronous `Mutex`, with one
//! major difference: the [`LockGuard`] returned by `lock` is not tied to the lifetime of the
//! `Mutex`. This enables you to acquire a lock, and then pass that guard into a future, and then
//! release it at some later point in time.
//!
//! This allows you to do something along the lines of:
//!
//! ```rust,no_run
//! #![feature(async_await)]
//!
//! use tokio::sync::Lock;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut data1 = Lock::new(0);
//!     let mut data2 = data1.clone();
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
//! [`Lock`]: struct.Lock.html
//! [`LockGuard`]: struct.LockGuard.html

use crate::semaphore;

use futures_core::ready;
use futures_util::future::poll_fn;
use std::cell::UnsafeCell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::task::Poll::Ready;
use std::task::{Context, Poll};

/// An asynchronous mutual exclusion primitive useful for protecting shared data
///
/// Each mutex has a type parameter (`T`) which represents the data that it is protecting. The data
/// can only be accessed through the RAII guards returned from `lock`, which
/// guarantees that the data is only ever accessed when the mutex is locked.
#[derive(Debug)]
pub struct Lock<T> {
    inner: Arc<State<T>>,
    permit: semaphore::Permit,
}

/// A handle to a held `Lock`.
///
/// As long as you have this guard, you have exclusive access to the underlying `T`. The guard
/// internally keeps a reference-couned pointer to the original `Lock`, so even if the lock goes
/// away, the guard remains valid.
///
/// The lock is automatically released whenever the guard is dropped, at which point `lock`
/// will succeed yet again.
#[derive(Debug)]
pub struct LockGuard<T>(Lock<T>);

// As long as T: Send, it's fine to send and share Lock<T> between threads.
// If T was not Send, sending and sharing a Lock<T> would be bad, since you can access T through
// Lock<T>.
unsafe impl<T> Send for Lock<T> where T: Send {}
unsafe impl<T> Sync for Lock<T> where T: Send {}
unsafe impl<T> Sync for LockGuard<T> where T: Send + Sync {}

#[derive(Debug)]
struct State<T> {
    c: UnsafeCell<T>,
    s: semaphore::Semaphore,
}

#[test]
fn bounds() {
    fn check<T: Send>() {}
    check::<LockGuard<u32>>();
}

impl<T> Lock<T> {
    /// Creates a new lock in an unlocked state ready for use.
    pub fn new(t: T) -> Self {
        Self {
            inner: Arc::new(State {
                c: UnsafeCell::new(t),
                s: semaphore::Semaphore::new(1),
            }),
            permit: semaphore::Permit::new(),
        }
    }

    fn poll_lock(&mut self, cx: &mut Context<'_>) -> Poll<LockGuard<T>> {
        ready!(self.permit.poll_acquire(cx, &self.inner.s)).unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        });

        // We want to move the acquired permit into the guard,
        // and leave an unacquired one in self.
        let acquired = Self {
            inner: self.inner.clone(),
            permit: ::std::mem::replace(&mut self.permit, semaphore::Permit::new()),
        };
        Ready(LockGuard(acquired))
    }

    /// A future that resolves on acquiring the lock and returns the `LockGuard`.
    #[allow(clippy::needless_lifetimes)] // false positive: https://github.com/rust-lang/rust-clippy/issues/3988
    pub async fn lock(&mut self) -> LockGuard<T> {
        poll_fn(|cx| self.poll_lock(cx)).await
    }
}

impl<T> Drop for LockGuard<T> {
    fn drop(&mut self) {
        if self.0.permit.is_acquired() {
            self.0.permit.release(&self.0.inner.s);
        } else if ::std::thread::panicking() {
            // A guard _should_ always hold its permit, but if the thread is already panicking,
            // we don't want to generate a panic-while-panicing, since that's just unhelpful!
        } else {
            unreachable!("Permit not held when LockGuard was dropped")
        }
    }
}

impl<T> From<T> for Lock<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

impl<T> Clone for Lock<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            permit: semaphore::Permit::new(),
        }
    }
}

impl<T> Default for Lock<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for LockGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        assert!(self.0.permit.is_acquired());
        unsafe { &*self.0.inner.c.get() }
    }
}

impl<T> DerefMut for LockGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        assert!(self.0.permit.is_acquired());
        unsafe { &mut *self.0.inner.c.get() }
    }
}

impl<T: fmt::Display> fmt::Display for LockGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
