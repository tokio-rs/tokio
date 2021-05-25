use super::batch_semaphore as ll; // low level implementation
use super::{AcquireError, TryAcquireError};
use std::sync::Arc;

/// Counting semaphore performing asynchronous permit acquisition.
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
///
/// This `Semaphore` is fair, which means that permits are given out in the order
/// they were requested. This fairness is also applied when `acquire_many` gets
/// involved, so if a call to `acquire_many` at the front of the queue requests
/// more permits than currently available, this can prevent a call to `acquire`
/// from completing, even if the semaphore has enough permits complete the call
/// to `acquire`.
///
/// To use the `Semaphore` in a poll function, you can use the [`PollSemaphore`]
/// utility.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::{Semaphore, TryAcquireError};
///
/// #[tokio::main]
/// async fn main() {
///     let semaphore = Semaphore::new(3);
///     {
///         let a_permit = semaphore.acquire().await;
///         let two_permits = semaphore.acquire_many(2).await;
///         assert_eq!(a_permit.is_ok(), true);
///         assert_eq!(two_permits.is_ok(), true);
///         // all three permits acquired without waiting
///         assert_eq!(semaphore.available_permits(), 0);
///         // any more `acquire()` or `acquire_many()` calls will wait
///         // `try_acquire` attempts to acquire, but fail immediately in this case
///         let fourth_permit = semaphore.try_acquire();
///         assert_eq!(fourth_permit.err(), Some(TryAcquireError::NoPermits));
///         semaphore.close();
///         // cannot obtain more permits after close
///         assert_eq!(semaphore.acquire().await.is_err(), true);
///     } // all permits are dropped at this point
///     assert_eq!(semaphore.available_permits(), 3);
///
///     // wrap a semaphore in [`Arc`] to share a semaphore across tasks
///     // use [`acquire_owned`] to move permits across tasks
///     let semaphore = Arc::new(Semaphore::new(3));
///     let mut join_handles = Vec::new();
///     for _ in 1..=5 {
///         let permit = semaphore.clone().acquire_owned().await.unwrap();
///         join_handles.push(tokio::spawn(async move {
///             // perform task...
///             // explicitly own `permit` in the task
///             drop(permit);
///         }));
///     }
///     assert_eq!(join_handles.len(), 5);
///     for j in join_handles {
///         j.await.unwrap();
///     }
/// }
/// ```
///
/// [`PollSemaphore`]: https://docs.rs/tokio-util/0.6/tokio_util/sync/struct.PollSemaphore.html
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
    permits: u32,
}

/// An owned permit from the semaphore.
///
/// This type is created by the [`acquire_owned`] method.
///
/// [`acquire_owned`]: crate::sync::Semaphore::acquire_owned()
#[must_use]
#[derive(Debug)]
pub struct OwnedSemaphorePermit {
    sem: Arc<Semaphore>,
    permits: u32,
}

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
    /// Creates a new semaphore with the initial number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            ll_sem: ll::Semaphore::new(permits),
        }
    }

    /// Creates a new semaphore with the initial number of permits.
    #[cfg(all(feature = "parking_lot", not(all(loom, test))))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    pub const fn const_new(permits: usize) -> Self {
        Self {
            ll_sem: ll::Semaphore::const_new(permits),
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

    /// Acquires a permit from the semaphore.
    ///
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`SemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    ///```
    /// use tokio::sync::Semaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(2);
    ///     let p1 = semaphore.acquire().await;
    ///     let p2 = semaphore.acquire().await;
    ///     assert_eq!(p1.is_ok(), true);
    ///     assert_eq!(p2.is_ok(), true);
    ///     assert_eq!(semaphore.available_permits(), 0);
    ///     semaphore.close();
    ///     let p3 = semaphore.acquire().await;
    ///     assert_eq!(p3.is_err(), true);
    /// }
    /// ```
    ///
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
        self.ll_sem.acquire(1).await?;
        Ok(SemaphorePermit {
            sem: &self,
            permits: 1,
        })
    }

    /// Acquires `n` permits from the semaphore.
    ///
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`SemaphorePermit`] representing the
    /// acquired permits.
    ///
    /// # Examples
    ///
    ///```
    /// use tokio::sync::Semaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(5);
    ///     let p1 = semaphore.acquire_many(3).await;
    ///     assert_eq!(semaphore.available_permits(), 2);
    ///     let p2 = semaphore.acquire_many(2).await;
    ///     assert_eq!(p1.is_ok(), true);
    ///     assert_eq!(p2.is_ok(), true);
    ///     semaphore.close();
    ///     let p3 = semaphore.acquire_many(99).await;
    ///     assert_eq!(p3.is_err(), true);
    /// }
    /// ```
    ///
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub async fn acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, AcquireError> {
        self.ll_sem.acquire(n).await?;
        Ok(SemaphorePermit {
            sem: &self,
            permits: n,
        })
    }

    /// Tries to acquire a permit from the semaphore.
    ///
    /// If the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left. Otherwise,
    /// this returns a [`SemaphorePermit`] representing the acquired permits.
    ///
    /// # Examples
    ///
    ///```
    /// use tokio::sync::{Semaphore, TryAcquireError};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(2);
    ///     let p1 = semaphore.acquire().await;
    ///     let p2 = semaphore.try_acquire();
    ///     let p3 = semaphore.try_acquire();
    ///     assert_eq!(p1.is_ok(), true);
    ///     assert_eq!(p2.is_ok(), true);
    ///     assert_eq!(p3.err(), Some(TryAcquireError::NoPermits));
    ///     semaphore.close();
    ///     let p4 = semaphore.try_acquire();
    ///     assert_eq!(p4.err(), Some(TryAcquireError::Closed));
    /// }
    /// ```
    ///
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                permits: 1,
            }),
            Err(e) => Err(e),
        }
    }

    /// Tries to acquire `n` permits from the semaphore.
    ///
    /// If the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left. Otherwise,
    /// this returns a [`SemaphorePermit`] representing the acquired permits.
    ///
    /// # Examples
    ///
    ///```
    /// use tokio::sync::{Semaphore, TryAcquireError};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(4);
    ///     let p1 = semaphore.try_acquire_many(3);
    ///     assert_eq!(semaphore.available_permits(), 1);
    ///     let p2 = semaphore.try_acquire_many(2);
    ///     assert_eq!(p1.is_ok(), true);
    ///     assert_eq!(p2.err(), Some(TryAcquireError::NoPermits));
    ///     semaphore.close();
    ///     let p3 = semaphore.try_acquire_many(99);
    ///     assert_eq!(p3.err(), Some(TryAcquireError::Closed));
    /// }
    /// ```
    ///
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub fn try_acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(n) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                permits: n,
            }),
            Err(e) => Err(e),
        }
    }

    /// Acquires a permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::Semaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(2));
    ///     let sem_clone = semaphore.clone();
    ///     let handle = tokio::spawn(async move {
    ///         let permit = sem_clone.acquire_owned().await;
    ///         assert_eq!(permit.is_ok(), true);
    ///     });
    ///     {
    ///         let p1 = semaphore.clone().acquire_owned().await;
    ///         assert_eq!(semaphore.available_permits(), 1);
    ///         let p2 = semaphore.clone().acquire_owned().await;
    ///         assert_eq!(semaphore.available_permits(), 0);
    ///         assert_eq!(p1.is_ok(), true);
    ///         assert_eq!(p2.is_ok(), true);
    ///     } // release the permits so the task can acquire them
    ///     handle.await.unwrap();
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.ll_sem.acquire(1).await?;
        Ok(OwnedSemaphorePermit {
            sem: self,
            permits: 1,
        })
    }

    /// Acquires `n` permits from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::Semaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(5));
    ///     let sem_clone = semaphore.clone();
    ///     let handle = tokio::spawn(async move {
    ///         let permit = sem_clone.acquire_many_owned(5).await;
    ///         assert_eq!(permit.is_ok(), true);
    ///     });
    ///     {
    ///         let p1 = semaphore.clone().acquire_many_owned(2).await;
    ///         assert_eq!(semaphore.available_permits(), 3);
    ///         let p2 = semaphore.clone().acquire_owned().await;
    ///         assert_eq!(semaphore.available_permits(), 2);
    ///         assert_eq!(p1.is_ok(), true);
    ///         assert_eq!(p2.is_ok(), true);
    ///     } // release the permits so the task can acquire them
    ///     handle.await.unwrap();
    /// }
    /// ```
    ///
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub async fn acquire_many_owned(
        self: Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.ll_sem.acquire(n).await?;
        Ok(OwnedSemaphorePermit {
            sem: self,
            permits: n,
        })
    }

    /// Tries to acquire a permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method. If
    /// the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left.
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::{Semaphore, TryAcquireError};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(2));
    ///     let sem_clone = semaphore.clone();
    ///     let handle = tokio::spawn(async move {
    ///         let permit = sem_clone.try_acquire_owned();
    ///         assert_eq!(permit.is_ok(), true);
    ///     });
    ///     {
    ///         let p1 = semaphore.clone().try_acquire_owned();
    ///         assert_eq!(semaphore.available_permits(), 1);
    ///         let p2 = semaphore.clone().try_acquire_owned();
    ///         assert_eq!(semaphore.available_permits(), 0);
    ///         let p3 = semaphore.clone().try_acquire_owned();
    ///         assert_eq!(p1.is_ok(), true);
    ///         assert_eq!(p2.is_ok(), true);
    ///         assert_eq!(p3.err(), Some(TryAcquireError::NoPermits));
    ///     } // release the permits so the task can acquire them
    ///     handle.await.unwrap();
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(OwnedSemaphorePermit {
                sem: self,
                permits: 1,
            }),
            Err(e) => Err(e),
        }
    }

    /// Tries to acquire `n` permits from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method. If
    /// the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left.
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::{Semaphore, TryAcquireError};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(5));
    ///     let sem_clone = semaphore.clone();
    ///     let handle = tokio::spawn(async move {
    ///         let permit = sem_clone.try_acquire_many_owned(5);
    ///         assert_eq!(permit.is_ok(), true);
    ///     });
    ///     {
    ///         let p1 = semaphore.clone().try_acquire_many_owned(3);
    ///         assert_eq!(semaphore.available_permits(), 2);
    ///         let p2 = semaphore.clone().try_acquire_many_owned(99);
    ///         assert_eq!(p1.is_ok(), true);
    ///         assert_eq!(p2.err(), Some(TryAcquireError::NoPermits));
    ///     } // release the permits so the task can acquire them
    ///     handle.await.unwrap();
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub fn try_acquire_many_owned(
        self: Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        match self.ll_sem.try_acquire(n) {
            Ok(_) => Ok(OwnedSemaphorePermit {
                sem: self,
                permits: n,
            }),
            Err(e) => Err(e),
        }
    }

    /// Closes the semaphore.
    ///
    /// This prevents the semaphore from issuing new permits and notifies all pending waiters.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Semaphore;
    /// use std::sync::Arc;
    /// use tokio::sync::TryAcquireError;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(1));
    ///     let semaphore2 = semaphore.clone();
    ///
    ///     tokio::spawn(async move {
    ///         let permit = semaphore.acquire_many(2).await;
    ///         assert!(permit.is_err());
    ///         println!("waiter received error");
    ///     });
    ///
    ///     println!("closing semaphore");
    ///     semaphore2.close();
    ///
    ///     // Cannot obtain more permits
    ///     assert_eq!(semaphore2.try_acquire().err(), Some(TryAcquireError::Closed))
    /// }
    /// ```
    pub fn close(&self) {
        self.ll_sem.close();
    }

    /// Returns true if the semaphore is closed
    pub fn is_closed(&self) -> bool {
        self.ll_sem.is_closed()
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

impl OwnedSemaphorePermit {
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

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        self.sem.add_permits(self.permits as usize);
    }
}
