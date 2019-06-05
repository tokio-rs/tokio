//! An asynchronous `Mutex`-like type.
//!
//! This module provides [`Lock`], a type that acts similarly to an asynchronous `Mutex`, with one
//! major difference: the [`LockGuard`] returned by `poll_lock` is not tied to the lifetime of the
//! `Mutex`. This enables you to acquire a lock, and then pass that guard into a future, and then
//! release it at some later point in time.
//!
//! This allows you to do something along the lines of:
//!
//! ```rust,no_run
//! # #[macro_use]
//! # extern crate futures;
//! # extern crate tokio;
//! # use futures::{future, Poll, Async, Future, Stream};
//! use tokio::sync::lock::{Lock, LockGuard};
//! struct MyType<S> {
//!     lock: Lock<S>,
//! }
//!
//! impl<S> Future for MyType<S>
//!   where S: Stream<Item = u32> + Send + 'static
//! {
//!     type Item = ();
//!     type Error = ();
//!
//!     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//!         match self.lock.poll_lock() {
//!             Async::Ready(mut guard) => {
//!                 tokio::spawn(future::poll_fn(move || {
//!                     let item = try_ready!(guard.poll().map_err(|_| ()));
//!                     println!("item = {:?}", item);
//!                     Ok(().into())
//!                 }));
//!                 Ok(().into())
//!             },
//!             Async::NotReady => Ok(Async::NotReady)
//!         }
//!     }
//! }
//! # fn main() {}
//! ```
//!
//!   [`Lock`]: struct.Lock.html
//!   [`LockGuard`]: struct.LockGuard.html

use futures::Async;
use semaphore;
use std::cell::UnsafeCell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// An asynchronous mutual exclusion primitive useful for protecting shared data
///
/// Each mutex has a type parameter (`T`) which represents the data that it is protecting. The data
/// can only be accessed through the RAII guards returned from `poll_lock`, which guarantees that
/// the data is only ever accessed when the mutex is locked.
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
/// The lock is automatically released whenever the guard is dropped, at which point `poll_lock`
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

    /// Try to acquire the lock.
    ///
    /// If the lock is already held, the current task is notified when it is released.
    pub fn poll_lock(&mut self) -> Async<LockGuard<T>> {
        if let Async::NotReady = self.permit.poll_acquire(&self.inner.s).unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        }) {
            return Async::NotReady;
        }

        // We want to move the acquired permit into the guard,
        // and leave an unacquired one in self.
        let acquired = Self {
            inner: self.inner.clone(),
            permit: ::std::mem::replace(&mut self.permit, semaphore::Permit::new()),
        };
        Async::Ready(LockGuard(acquired))
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
