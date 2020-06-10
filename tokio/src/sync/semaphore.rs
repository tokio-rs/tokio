use super::batch_semaphore as ll; // low level implementation
use std::sync::Arc;

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

/// A permit from the semaphore.
///
/// This type is created by the [`acquire`] method.
///
/// [`acquire`]: crate::sync::Semaphore::acquire()
#[must_use]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    sem: &'a Semaphore,
}

/// An owned permit from the semaphore.
///
/// This type is created by the [`acquire_owned`] method.
///
/// [`acquire_owned`]: crate::sync::Semaphore::acquire_owned()
#[must_use]
#[derive(Debug)]
pub struct OwnedSemaphorePermit {
    sem: Option<Arc<Semaphore>>,
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

#[test]
fn is_slim() {
    assert_eq!(std::mem::size_of::<&()>(), std::mem::size_of::<SemaphorePermit<'static>>());
    assert_eq!(std::mem::size_of::<Arc<()>>(), std::mem::size_of::<OwnedSemaphorePermit>());
}

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            ll_sem: ll::Semaphore::new(permits),
        }
    }

    /// Returns the current number of available permits.
    pub fn available_permits(&self) -> usize {
        self.ll_sem.available_permits()
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// The maximum number of permits is `usize::MAX >> 3`, and this function will panic if the limit is exceeded.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.release(n);
    }

    /// Acquires permit from the semaphore.
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        self.ll_sem.acquire(1).await.unwrap();
        SemaphorePermit {
            sem: &self,
        }
    }

    /// Tries to acquire a permit from the semaphore.
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
            }),
            Err(_) => Err(TryAcquireError(())),
        }
    }

    /// Acquires permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    ///
    /// [`Arc`]: std::sync::Arc
    pub async fn acquire_owned(self: Arc<Self>) -> OwnedSemaphorePermit {
        self.ll_sem.acquire(1).await.unwrap();
        OwnedSemaphorePermit {
            sem: Some(self.clone()),
        }
    }

    /// Tries to acquire a permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    ///
    /// [`Arc`]: std::sync::Arc
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(OwnedSemaphorePermit {
                sem: Some(self.clone()),
            }),
            Err(_) => Err(TryAcquireError(())),
        }
    }
}

impl<'a> SemaphorePermit<'a> {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(self) {
        // the destructor won't run, so it won't release the permits.
        // there are no fields that could leak.
        std::mem::forget(self);
    }
}

impl OwnedSemaphorePermit {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(mut self) {
        // the destructor won't be able to release the permits
        self.sem = None;
    }
}

impl<'a> Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.sem.add_permits(1);
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        if let Some(ref mut sem) = self.sem {
            sem.add_permits(1);
        }
    }
}
