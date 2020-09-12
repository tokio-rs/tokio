//! Signal driver

use crate::park::Park;
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Responsible for registering wakeups when an OS signal is received, and
/// subsequently dispatching notifications to any signal listeners as appropriate.
///
/// Note: this driver relies on having an enabled IO driver in order to listen to
/// pipe write wakeups.
#[derive(Debug)]
pub(crate) struct Driver<T: Park> {
    /// Shared state
    inner: Arc<Inner>,

    /// Thread parker. The `Driver` park implementation delegates to this.
    park: T,
}

#[derive(Clone, Debug)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

#[derive(Debug)]
struct Inner(());

// ===== impl Driver =====

impl<T> Driver<T>
where
    T: Park,
{
    /// Creates a new signal `Driver` instance that delegates wakeups to `park`.
    pub(crate) fn new(park: T) -> Self {
        Self {
            inner: Arc::new(Inner::new()),
            park,
        }
    }

    /// Returns a handle to this event loop which can be sent across threads
    /// and can be used as a proxy to the event loop itself.
    pub(crate) fn handle(&self) -> Handle {
        Handle {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

// ===== impl Park for Driver =====

impl<T> Park for Driver<T>
where
    T: Park,
{
    type Unpark = T::Unpark;
    type Error = T::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.park.park()
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.park.park_timeout(duration)
    }

    fn shutdown(&mut self) {
        self.park.shutdown()
    }
}

impl<T> Drop for Driver<T>
where
    T: Park,
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ===== impl Inner =====

impl Inner {
    fn new() -> Self {
        Self(())
    }
}
