use super::batch_semaphore as ll; // low level implementation
use crate::coop::CoopFutureExt;

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
    permits: u16,
}

/// Error returned from the [`Semaphore::try_acquire`] function.
///
/// A `try_acquire` operation can only fail if the semaphore has no available
/// permits.
///
/// [`Semaphore::try_acquire`]: Semaphore::try_acquire
#[derive(Debug)]
pub struct TryAcquireError(());

#[test]
#[cfg(not(loom))]
fn bounds() {
    fn check_unpin<T: Unpin>() {}
    // This has to take a value, since the async fn's return type is unnameable.
    fn check_send_sync_val<T: Send + Sync>(_t: T) {}
    fn check_send_sync<T: Send + Sync>() {}
    check_unpin::<Semaphore>();
    check_unpin::<SemaphorePermit<'_>>();
    check_send_sync::<Semaphore>();

    let semaphore = Semaphore::new(0);
    check_send_sync_val(semaphore.acquire());
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

    /// Adds `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.release(n);
    }

    /// Acquires permit from the semaphore
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        self.ll_sem.acquire(1).cooperate().await.unwrap();
        SemaphorePermit {
            sem: &self,
            permits: 1,
        }
    }

    /// Tries to acquire a permit form the semaphore
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                permits: 1,
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
        self.permits = 0;
    }
}

impl<'a> Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.sem.add_permits(self.permits as usize);
    }
}
