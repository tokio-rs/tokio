//! Thread-safe, asynchronous counting semaphore.
//!
//! A `Semaphore` instance holds a set of permits. Permits are used to
//! synchronize access to a shared resource.
//!
//! Before accessing the shared resource, callers acquire a permit from the
//! semaphore. Once the permit is acquired, the caller then enters the critical
//! section. If no permits are available, then the task can wait until a
//! permit becomes available.

use std::error::Error;
use std::fmt;

use super::semaphore_ll as ll;
use crate::future::poll_fn; // low level implementation

/// An enumeration of possible errors which can occur while trying to
/// aquire a permit using the `Semaphore::acquire` function.
#[derive(Debug)]
pub enum AcquireError {
    /// The semaphore is closed and no new permits can be acquired.
    Closed,
}

impl From<ll::AcquireError> for AcquireError {
    fn from(_e: ll::AcquireError) -> Self {
        Self::Closed
    }
}

impl fmt::Display for AcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match self {
                Self::Closed => "semaphore is closed",
            }
        )
    }
}

impl Error for AcquireError {}

/// An enumeration of possible errors which can occur while trying to
/// aquire a permit using the `Semaphore::try_acquire` function.
#[derive(Debug)]
pub enum TryAcquireError {
    /// The semaphore is closed and no new permits can be acquired.
    Closed,
    /// There are currently no permits available.
    NoPermits,
}

impl fmt::Display for TryAcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match self {
                Self::Closed => "semaphore closed",
                Self::NoPermits => "no permits available",
            }
        )
    }
}

impl From<ll::TryAcquireError> for TryAcquireError {
    fn from(e: ll::TryAcquireError) -> Self {
        match e.kind {
            ll::ErrorKind::Closed => Self::Closed,
            ll::ErrorKind::NoPermits => Self::NoPermits,
        }
    }
}

impl Error for TryAcquireError {}

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

    /// Close the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub fn close(&self) {
        self.ll_sem.close();
    }

    /// Add `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.add_permits(n);
    }

    /// Remove `n` permits from the semaphore.
    ///
    /// This function acquires `n` permits from the semaphore and calls
    /// `forget` for them.
    ///
    /// # Warning
    ///
    /// If the semaphore has less permits than the given `n` this function
    /// will never return and wait for new permits to be added until `n`
    /// permits were removed from the semaphore.
    pub async fn remove_permits(&self, n: usize) -> Result<(), AcquireError> {
        for _ in 0..n {
            let permit = self.acquire().await?;
            permit.forget();
        }
        Ok(())
    }

    /// Tries to remove up to `n` permits from the semaphore and returns
    /// the number of removed semaphores.
    pub fn try_remove_permits(&self, n: usize) -> usize {
        for i in 0..n {
            match self.try_acquire() {
                Ok(permit) => permit.forget(),
                Err(_) => return i,
            }
        }
        n
    }

    /// Aquire permit from the semaphore
    pub async fn acquire(&self) -> Result<Permit<'_>, AcquireError> {
        let mut permit = Permit {
            sem: &self,
            ll_permit: ll::Permit::new(),
        };
        poll_fn(|cx| permit.ll_permit.poll_acquire(cx, &self.ll_sem)).await?;
        Ok(permit)
    }

    /// Try to aquire a permit form the semaphore
    pub fn try_acquire(&self) -> Result<Permit<'_>, TryAcquireError> {
        let mut ll_permit = ll::Permit::new();
        ll_permit.try_acquire(&self.ll_sem)?;
        Ok(Permit {
            sem: self,
            ll_permit,
        })
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
