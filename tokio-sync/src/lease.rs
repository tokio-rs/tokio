//! An asynchronous, atomic option type.
//!
//! This module provides `Lease`, a type that acts similarly to an asynchronous `Mutex`, with one
//! major difference: it expects you to move the leased item _by value_, and then _return it_ when
//! you are done. You can think of a `Lease` as an atomic, asynchronous `Option` type, in which we
//! can `take` the value only if no-one else has currently taken it, and where we are notified when
//! the value has returned so we can try to take it again.
//!
//! This type is intended for use with methods that take `self` by value, and _eventually_, at some
//! later point in time, return that `Self` for future use. For example, consider the following
//! method for a hypothetical, non-pipelined connection type:
//!
//! ```rust,ignore
//! impl Connection {
//!     fn get(self, key: i64) -> impl Future<Item = (i64, Self), Error = Error>;
//! }
//! ```
//!
//! Let's say you want to expose an interface that does _not_ consume `self`, but instead has a
//! `poll_ready` method that checks whether the connection is ready to receive another request:
//!
//! ```rust,ignore
//! impl MyConnection {
//!     fn poll_ready(&mut self) -> Poll<(), Error = Error>;
//!     fn call(&mut self, key: i64) -> impl Future<Item = i64, Error = Error>;
//! }
//! ```
//!
//! `Lease` allwos you to do this. Specifically, `poll_ready` attempts to acquire the lease using
//! `Lease::poll_acquire`, and `call` _transfers_ that lease into the returned future. When the
//! future eventually resolves, we _restore_ the leased value so that `poll_ready` returns `Ready`
//! again to anyone else who may want to take the value. The example above would thus look like
//! this:
//!
//! ```rust,ignore
//! impl MyConnection {
//!     fn poll_ready(&mut self) -> Poll<(), Error = Error> {
//!         self.lease.poll_acquire()
//!     }
//!
//!     fn call(&mut self, key: i64) -> impl Future<Item = i64, Error = Error> {
//!         // We want to transfer the lease into the future
//!         // and leave behind an unacquired lease.
//!         let mut lease = self.lease.transfer();
//!         lease.take().get(key).map(move |(v, connection)| {
//!             // Give back the connection for other callers.
//!             // After this, `poll_ready` may return `Ok(Ready)` again.
//!             lease.restore(connection);
//!             // And yield just the value.
//!             v
//!         })
//!     }
//! }
//! ```

use futures::Async;
use semaphore;
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// A handle to a leasable value.
///
/// Use `poll_acquire` to acquire the lease, `take` to grab the leased value, and `restore` to
/// return the leased value when you're done with it.
///
/// The code will panic if you attempt to access the `S` behind a `Lease` through `Deref` or
/// `DerefMut` without having acquired the lease through `poll_acquire` first, or if you have
/// called `take`.
///
/// The code will also panic if you attempt to drop a `Lease` without first restoring the leased
/// value.
#[derive(Debug)]
pub struct Lease<T> {
    inner: Arc<State<T>>,
    permit: semaphore::Permit,
}

// As long as T: Send, it's fine to send Lease<T> to other threads.
// If T was not Send, sending a Lease<T> would be bad, since you can access T through Lease<T>.
unsafe impl<T> Send for Lease<T> where T: Send {}

#[derive(Debug)]
struct State<T> {
    c: UnsafeCell<Option<T>>,
    s: semaphore::Semaphore,
}

impl<T> Lease<T> {
    fn option(&mut self) -> &mut Option<T> {
        unsafe { &mut *self.inner.c.get() }
    }

    /// Try to acquire the lease.
    ///
    /// If the lease is not available, the current task is notified once it is.
    pub fn poll_acquire(&mut self) -> Async<()> {
        self.permit.poll_acquire(&self.inner.s).unwrap_or_else(|_| {
            // The semaphore was closed. but, we never explicitly close it, and we have a
            // handle to it through the Arc, which means that this can never happen.
            unreachable!()
        })
    }

    /// Release the lease (if it has been acquired).
    ///
    /// This provides a way of "undoing" a call to `poll_ready` should you decide you don't need
    /// the lease after all, or if you only needed access by reference.
    ///
    /// This method will panic if you attempt to release the lease after you have called `take`.
    pub fn release(&mut self) {
        if self.permit.is_acquired() {
            // We need this check in case the reason we get here is that we already hit this
            // assertion, and `release` is being called _again_ because `self` is being dropped.
            if !::std::thread::panicking() {
                assert!(
                    self.option().is_some(),
                    "attempted to release the lease without restoring the value"
                );
            }
            self.permit.release(&self.inner.s);
        }
    }

    /// Leave behind a non-acquired lease in place of this one, and return this acquired lease.
    ///
    /// This allows you to move a previously acquired lease into another context (like a `Future`)
    /// where you will later `restore` the leased value.
    ///
    /// This method will panic if you attempt to call it without first having acquired the lease.
    pub fn transfer(&mut self) -> Self {
        assert!(self.permit.is_acquired());
        let mut transferred = self.clone();
        ::std::mem::swap(self, &mut transferred);
        transferred
    }

    /// Take the leased value.
    ///
    /// This method will panic if you attempt to call it without first having acquired the lease.
    ///
    /// Note that you _must_ call `restore` on this lease before you drop it to return the leased
    /// value to other waiting clients. If you do not, dropping the lease will panic.
    pub fn take(&mut self) -> T {
        assert!(self.permit.is_acquired());
        self.option()
            .take()
            .expect("attempted to call take(), but leased value has not been restored")
    }

    /// Restore the leased value.
    ///
    /// This method will panic if you attempt to call it without first having acquired the lease.
    ///
    /// Note that once you return the leased value, the lease is no longer considered acquired.
    pub fn restore(&mut self, state: T) {
        assert!(self.permit.is_acquired());
        unsafe { *self.inner.c.get() = Some(state) };
        // Finally, we can now release the permit since we're done with the connection
        self.release();
    }
}

impl<T> Drop for Lease<T> {
    fn drop(&mut self) {
        self.release();
    }
}

impl<T> Deref for Lease<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        assert!(self.permit.is_acquired());
        let s = unsafe { &*self.inner.c.get() };
        s.as_ref()
            .expect("attempted to deref lease after calling take()")
    }
}

impl<T> DerefMut for Lease<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        assert!(self.permit.is_acquired());
        let s = unsafe { &mut *self.inner.c.get() };
        s.as_mut()
            .expect("attempted to deref_mut lease after calling take()")
    }
}

impl<T> From<T> for Lease<T> {
    fn from(s: T) -> Self {
        Self {
            inner: Arc::new(State {
                c: UnsafeCell::new(Some(s)),
                s: semaphore::Semaphore::new(1),
            }),
            permit: semaphore::Permit::new(),
        }
    }
}

impl<T> Clone for Lease<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            permit: semaphore::Permit::new(),
        }
    }
}

impl<T> Default for Lease<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::from(T::default())
    }
}
