//! Thread-safe, asynchronous counting semaphore.
//!
//! A `Semaphore` instance holds a set of permits. Permits are used to
//! synchronize access to a shared resource.
//!
//! Before accessing the shared resource, callers acquire a permit from the
//! semaphore. Once the permit is acquired, the caller then enters the critical
//! section. If no permits are available, then the task can wait until a
//! permit becomes available.

use super::semaphore_ll as ll; // low level implementation
use crate::future::poll_fn;

/// Futures-aware semaphore
#[derive(Debug)]
pub struct Semaphore {
    /// The low level semaphore
    ll_sem: ll::Semaphore,
}

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    pub fn new(permits: usize) -> Self {
        Self {
            ll_sem: ll::Semaphore::new(permits),
        }
    }

    /// Returns the current number of available permits
    pub fn available_permits(&self) -> usize {
        self.ll_sem.available_permits()
    }

    /// Add `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.add_permits(n);
    }

    /// Acquire permit from the semaphore
    pub async fn acquire(&self) -> Permit<'_> {
        let mut permit = Permit {
            sem: &self,
            ll_permit: ll::Permit::new(),
        };
        poll_fn(|cx| permit.ll_permit.poll_acquire(cx, &self.ll_sem)).await.unwrap();
        permit
    }

    /// Try to acquire a permit form the semaphore
    pub fn try_acquire(&self) -> Option<Permit<'_>> {
        let mut ll_permit = ll::Permit::new();
        match ll_permit.try_acquire(&self.ll_sem) {
            Ok(_) => Some(Permit { sem: self, ll_permit }),
            Err(_) => None,
        }

    }
}

/// A permit from the semaphore
#[must_use]
#[derive(Debug)]
pub struct Permit<'a> {
    sem: &'a Semaphore,
    // the low level permit
    ll_permit: ll::Permit,
}

impl<'a> Permit<'a> {
    /// Forget the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(mut self) {
        self.ll_permit.forget();
    }
}

impl<'a> Drop for Permit<'_> {
    fn drop(&mut self) {
        self.ll_permit.release(&self.sem.ll_sem);
    }
}
