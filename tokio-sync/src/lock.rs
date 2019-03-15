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
//! # extern crate futures;
//! # use futures::{Poll, Async, Future, Sink};
//! # mod tokio { pub fn spawn<F: ::futures::Future<Item = (), Error = ()>>(f: F) {} }
//! use tokio_sync::lock::Lock;
//! struct MyType<S> {
//!     lock: Lock<S>,
//! }
//!
//! impl<S> Future for MyType<S>
//!   where S: Sink<SinkItem = u32>
//! {
//! # type Item = ();
//! # type Error = ();
//!     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//!         match self.lock.poll_lock() {
//!             Async::Ready(guard) => {
//!                 tokio::spawn(guard.send(42).then(|_| Ok(())));
//!                 Ok(Async::Ready(()))
//!             },
//!             Async::NotReady => Ok(Async::NotReady)
//!         }
//!     }
//! }
//! ```
//!
//!   [`Lock`]: struct.Lock.html
//!   [`LockGuard`]: struct.LockGuard.html

use futures::{Async, Future, Poll, Sink, StartSend, Stream};
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

// As long as T: Send, it's fine to send Lock<T> to other threads.
// If T was not Send, sending a Lock<T> would be bad, since you can access T through Lock<T>.
unsafe impl<T> Send for Lock<T> where T: Send {}
unsafe impl<T> Sync for LockGuard<T> where T: Send + Sync {}

#[derive(Debug)]
struct State<T> {
    c: UnsafeCell<T>,
    s: semaphore::Semaphore,
}

impl<T> Lock<T> {
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
        Self {
            inner: Arc::new(State {
                c: UnsafeCell::new(s),
                s: semaphore::Semaphore::new(1),
            }),
            permit: semaphore::Permit::new(),
        }
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
        Self::from(T::default())
    }
}

// === forwarding traits for LockGuard<T> ===

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

// NOTE: it'd be really nice to forward AsyncRead/AsyncWrite too

impl<T: fmt::Display> fmt::Display for LockGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: Stream> Stream for LockGuard<T> {
    type Item = T::Item;
    type Error = T::Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        T::poll(&mut **self)
    }
}

impl<T: Sink> Sink for LockGuard<T> {
    type SinkItem = T::SinkItem;
    type SinkError = T::SinkError;
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        T::start_send(&mut **self, item)
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        T::poll_complete(&mut **self)
    }
}

impl<T: Future> Future for LockGuard<T> {
    type Item = T::Item;
    type Error = T::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        T::poll(&mut **self)
    }
}
