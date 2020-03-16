use super::semaphore_ll as ll; // low level implementation
use crate::future::poll_fn;

/// Counting semaphore performing asynchronous permit aquisition.
///
/// A semaphore maintains a set of permits. Permits are used to synchronize
/// access to a shared resource. A semaphore differs from a mutex in that it
/// can allow more than one concurrent caller to access the shared resource at a
/// time.
///
/// When `acquire` is called and the semaphore has remaining permits, the
/// function immediately returns a permit. However, if no remaining permits are
/// available, `acquire` (asynchronously) waits until an outstanding permit is
/// dropped. At this point, the freed permit is assigned to the caller.
#[derive(Debug)]
pub struct Semaphore {
    /// The low level semaphore
    ll_sem: ll::Semaphore,
}

/// A permit from the semaphore
#[must_use]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    sem: &'a Semaphore,
    // the low level permit
    ll_permit: ll::Permit,
}

/// Error returned from the [`Semaphore::try_acquire`] function.
///
/// A `try_acquire` operation can only fail if the semaphore has no available
/// permits.
///
/// [`Semaphore::try_acquire`]: Semaphore::try_acquire
#[derive(Debug)]
pub struct TryAcquireError(());

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

    /// Adds `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.add_permits(n);
    }

    /// Acquires permit from the semaphore
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        let mut permit = SemaphorePermit {
            sem: &self,
            ll_permit: ll::Permit::new(),
        };
        poll_fn(|cx| {
            // Keep track of task budget
            ready!(crate::coop::poll_proceed(cx));

            permit.ll_permit.poll_acquire(cx, 1, &self.ll_sem)
        })
        .await
        .unwrap();
        permit
    }

    /// Tries to acquire a permit form the semaphore
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        let mut ll_permit = ll::Permit::new();
        match ll_permit.try_acquire(1, &self.ll_sem) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                ll_permit,
            }),
            Err(_) => Err(TryAcquireError(())),
        }
    }
}

impl<'a> SemaphorePermit<'a> {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(mut self) {
        self.ll_permit.forget(1);
    }
}

impl<'a> Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.ll_permit.release(1, &self.sem.ll_sem);
    }
}
