use super::Semaphore;
use crate::{loom::cell::UnsafeCell, sync::SemaphorePermit};
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::Drop;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

enum LazyUninitData<Fut, F> {
    Function(ManuallyDrop<F>),
    Future(ManuallyDrop<Fut>),
}

/// The inner data of a `Lazy`.
/// Is `function` when not yet initialized,
/// `value` with `Some` when initialized,
/// and `value` with `None` when poisoned.
union LazyData<T, F, Fut> {
    init: ManuallyDrop<Option<T>>,
    uninit: ManuallyDrop<LazyUninitData<F, Fut>>,
}

/// A value that is initialized on the first access.
/// The initialization procedure is allowed to be asycnhronous,
/// so access to the value requires an `await`.
///
/// # Example
///
/// ```
/// use tokio::sync::Lazy;
///
/// async fn some_computation() -> u32 {
///     1 + 1
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let lazy : Lazy<u32> = Lazy::const_new(|| Box::pin(async { some_computation().await }));
///
///     let result = tokio::spawn(async move {
///         *lazy.force().await
///     }).await.unwrap();
///
///     assert_eq!(result, 2);
/// }
/// ```
pub struct Lazy<T, Fut = Pin<Box<dyn Future<Output = T> + Send>>, F = fn() -> Fut> {
    value_set: AtomicBool,
    value: UnsafeCell<LazyData<T, Fut, F>>,
    semaphore: Semaphore,
}

impl<T, Fut, F> Lazy<T, Fut, F> {
    /// Creates a new empty `Lazy` instance with the given initializing
    /// async function.
    #[must_use]
    #[inline]
    pub fn new(init: F) -> Self {
        Self {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(LazyData {
                uninit: ManuallyDrop::new(LazyUninitData::Function(ManuallyDrop::new(init))),
            }),
            semaphore: Semaphore::new(1),
        }
    }

    /// Creates a new `Lazy` instance with the given initializing
    /// async function.
    ///
    /// Equivalent to `Lazy::new`, except that it can be used in static
    /// variables.
    /// /// # Example
    ///
    /// ```
    /// use tokio::sync::Lazy;
    ///
    /// async fn some_computation() -> u32 {
    ///     1 + 1
    /// }
    ///
    /// static LAZY : Lazy<u32> = Lazy::const_new(|| Box::pin(async { some_computation().await }));
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = tokio::spawn(async {
    ///         *LAZY.force().await
    ///     }).await.unwrap();
    ///
    ///     assert_eq!(result, 2);
    /// }
    /// ```
    #[cfg(all(feature = "parking_lot", not(all(loom, test))))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    #[must_use]
    #[inline]
    pub const fn const_new(init: F) -> Self {
        Self {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(LazyData {
                uninit: ManuallyDrop::new(LazyUninitData::Function(ManuallyDrop::new(init))),
            }),
            semaphore: Semaphore::const_new(1),
        }
    }

    /// Returns `true` if this `Lazy` has been initialized, and `false` otherwise.
    #[must_use]
    #[inline]
    fn initialized(&self) -> bool {
        self.value_set.load(Ordering::Acquire)
    }

    /// Returns `true` if this `Lazy` has been initialized, and `false` otherwise.
    /// Because it takes a mutable reference, it doesn't require an atomic operation.
    #[must_use]
    #[inline]
    fn initialized_mut(&mut self) -> bool {
        *self.value_set.get_mut()
    }

    /// Returns `true` if this `Lazy` has been initialized, and `false` otherwise.
    /// Because it takes a mutable reference, it doesn't require an atomic operation.
    #[must_use]
    #[inline]
    fn initialized_pin_mut(self: Pin<&mut Self>) -> bool {
        // SAFETY: this doens't move out of any pinned
        unsafe { *self.get_unchecked_mut().value_set.get_mut() }
    }

    /// Returns a reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    #[must_use]
    #[inline]
    unsafe fn get_unchecked(&self) -> Option<&T> {
        unsafe { self.value.with(|v| (*v).init.as_ref()) }
    }

    /// Returns a mutable reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    #[must_use]
    #[inline]
    unsafe fn get_unchecked_mut(&mut self) -> Option<&mut T> {
        #[allow(clippy::needless_borrow)]
        unsafe {
            self.value.with_mut(|v| (&mut *v).init.as_mut())
        }
    }

    /// Returns a pinned reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    #[must_use]
    #[inline]
    unsafe fn get_unchecked_pin(self: Pin<&Self>) -> Option<Pin<&T>> {
        unsafe {
            self.value
                .with(|v| (*v).init.as_ref().map(|r| Pin::new_unchecked(r)))
        }
    }

    /// Returns a pinned mutable reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    #[must_use]
    #[inline]
    unsafe fn get_unchecked_pin_mut(self: Pin<&mut Self>) -> Option<Pin<&mut T>> {
        #[allow(clippy::needless_borrow)]
        unsafe {
            self.value
                .with_mut(|v| (&mut *v).init.as_mut().map(|r| Pin::new_unchecked(r)))
        }
    }

    /// Gets a reference to the result of this lazy value if it was initialized,
    /// otherwise returns `None`.
    #[must_use]
    #[inline]
    pub fn get(&self) -> Option<&T> {
        if self.initialized() {
            // SAFETY: we just checked that the `Lazy` is initialized
            unsafe { self.get_unchecked() }
        } else {
            None
        }
    }

    /// Gets a mutable reference to the result of this lazy value if it was initialized,
    /// otherwise returns `None`.
    #[must_use]
    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.initialized_mut() {
            // SAFETY: we just checked that the `Lazy` is initialized
            unsafe { self.get_unchecked_mut() }
        } else {
            None
        }
    }

    /// Gets a pinned reference to the result of this lazy value if it was initialized,
    /// otherwise returns `None`.
    #[must_use]
    #[inline]
    pub fn get_pin(self: Pin<&Self>) -> Option<Pin<&T>> {
        if self.initialized() {
            // SAFETY: we just checked that the `Lazy` is initialized
            unsafe { self.get_unchecked_pin() }
        } else {
            None
        }
    }

    /// Gets a pinned mutable reference to the result of this lazy value if it was initialized,
    /// otherwise returns `None`.
    #[must_use]
    #[inline]
    pub fn get_pin_mut(mut self: Pin<&mut Self>) -> Option<Pin<&mut T>> {
        if self.as_mut().initialized_pin_mut() {
            // SAFETY: we just checked that the `Lazy` is initialized
            unsafe { self.get_unchecked_pin_mut() }
        } else {
            None
        }
    }
}

impl<T, Fut, F> Lazy<T, Fut, F>
where
    Fut: Future<Output = T> + Unpin,
    F: FnOnce() -> Fut,
{
    /// Forces the evaluation of this lazy value and returns a reference to the result.
    ///
    /// # Panics
    ///
    /// If the initialization function panics, the `Lazy` is poisoned
    /// and all subsequent calls to this function will panic.
    #[inline]
    pub async fn force(&self) -> &T {
        if !self.initialized() {
            // SAFETY: the cell is not initialized, so it stores `F` or `Fut`.
            // And `F` is `Unpin`. So making a pinned reference is safe.
            unsafe {
                Pin::new_unchecked(self).initialize().await;
            }
        }
        // SAFETY: we just initialized the `Lazy`
        (unsafe { self.get_unchecked() })
            .unwrap_or_else(|| panic!("Lazy instance has previously been poisoned"))
    }

    /// Forces the evaluation of this lazy value and returns a mutable reference to the result.
    ///
    /// # Panics
    ///
    /// If the initialization function panics, the `Lazy` is poisoned
    /// and all subsequent calls to this function will panic.
    #[inline]
    pub async fn force_mut(&mut self) -> &mut T {
        if !self.initialized_mut() {
            // SAFETY: the cell is not initialized, so it stores `F` or `Fut`.
            // And `F` is `Unpin`. So making a pinned reference is safe.
            unsafe {
                Pin::new_unchecked(&*self).initialize().await;
            }
        }
        // SAFETY: we just initialized the `Lazy`
        (unsafe { self.get_unchecked_mut() })
            .unwrap_or_else(|| panic!("Lazy instance has previously been poisoned"))
    }
}

impl<T, Fut, F> Lazy<T, Fut, F>
where
    Fut: Future<Output = T>,
    F: FnOnce() -> Fut,
{
    /// Initializes the `Lazy`, if it has not yet been.
    /// Once this has returned, `LazyData` is guaranteed to be in the `value` state.
    #[cold]
    async fn initialize(self: Pin<&Self>) {
        // Here we try to acquire the semaphore permit. Holding the permit
        // will allow us to set the value of the Lazy, and prevents
        // other tasks from initializing the Lazy while we are holding
        // it.
        match self.semaphore.acquire().await {
            Ok(permit) => {
                debug_assert!(!self.initialized());

                enum InitializationState<T> {
                    // The `Lazy` stores the initializing future.
                    Initializing,
                    // The `Lazy` has been fully initialized,
                    // or poisoned if this contains `None`.
                    Initialized(ManuallyDrop<Option<T>>),
                }

                // If the function panics, we have to set the `Lazy` to poisoned.
                // So all the initialization work happens on the `Drop` of this struct.
                struct InitializeOnDrop<'a, 'permit, T, Fut, F> {
                    lazy: Pin<&'a Lazy<T, Fut, F>>,
                    value: InitializationState<T>,
                    permit: ManuallyDrop<SemaphorePermit<'permit>>,
                }

                impl<'a, 'permit, T, Fut, F> Drop for InitializeOnDrop<'a, 'permit, T, Fut, F> {
                    fn drop(&mut self) {
                        match self.value {
                            InitializationState::Initializing => {
                                // Dropped while initializing -
                                // release the permit, give next caller a chance.
                                // At this point, the `Lazy` stores the initializing future.
                                // SAFETY: we are in `drop`, so nobody will access our fields after us.
                                unsafe {
                                    drop(ManuallyDrop::take(&mut self.permit));
                                }
                            }
                            InitializationState::Initialized(ref mut value) => {
                                // Write the initialized value to the `Lazy`.
                                // SAFETY: we are in `drop`, so nobody will access our fields after us.
                                unsafe {
                                    self.lazy.value.with_mut(|v| {
                                        (*v).init = ManuallyDrop::new(ManuallyDrop::take(value))
                                    });
                                }

                                // FIXME this could be made unsychronized when we have a mutable borrow of the `Lazy`
                                self.lazy.value_set.store(true, Ordering::Release);
                                self.lazy.semaphore.close();
                            }
                        }
                    }
                }

                // SAFETY: This lazy is uninitialized, so it still stores either the initializing function
                // or the initializing future.
                let uninit_data = unsafe { self.value.with_mut(|v| &mut (*v).uninit) };

                // Arm the initialize-on-drop-mechanism.
                // We start as `poisoned` because if the initialization function panics,
                // we want the `Lazy` to be poisoned.
                let mut initialize_on_drop = InitializeOnDrop {
                    lazy: self,
                    value: InitializationState::Initialized(ManuallyDrop::new(None)),
                    permit: ManuallyDrop::new(permit),
                };

                // We need to hold a raw pointer across an `await`, without impacting auto traits.
                // Hence this ugliness.
                struct Wrapper<Fut>(*mut Fut);
                unsafe impl<Fut> Send for Wrapper<Fut> {}
                unsafe impl<Fut> Sync for Wrapper<Fut> {}

                let fut_ptr: Wrapper<Fut> = {
                    let mut_ptr: *mut ManuallyDrop<Fut> = match &mut **uninit_data {
                        LazyUninitData::Function(f) => {
                            // SAFETY: The `f` will never be accessed later
                            let f = unsafe { ManuallyDrop::take(f) };

                            // Run the initializing function.
                            let fut = f();

                            **uninit_data = LazyUninitData::Future(ManuallyDrop::new(fut));

                            match &mut **uninit_data {
                                // Someone else already ran the intializing function.
                                // Get a pointer to the future that this `Lazy` is currently storing.
                                LazyUninitData::Future(fut) => fut,
                                _ => unreachable!("We just set this to LazyUninitData::Future"),
                            }
                        }
                        LazyUninitData::Future(fut) => fut,
                    };

                    Wrapper(mut_ptr.cast())
                };

                // SAFETY: `Lazy` futures are structurally pinned.
                let fut: Pin<&mut Fut> = unsafe { Pin::new_unchecked(&mut *fut_ptr.0) };

                // If we reach this point, the initializing function has run successfully,
                // and the initializing future is stored in the struct.
                // Now we disarm the poison mechanism before polling the future.
                initialize_on_drop.value = InitializationState::Initializing;

                // `await` the initializing future.
                // If this panics or we are cancelled,
                // the semaphore permit will be released and the next
                // caller to `initialize` will keep polling where we left off.
                let result = fut.await;

                // Set the cell to initialized.
                initialize_on_drop.value =
                    InitializationState::Initialized(ManuallyDrop::new(Some(result)));

                // Drop the future now that we are done polling it.
                // SAFETY: there are no accesses to the future after this.
                unsafe { core::ptr::drop_in_place(fut_ptr.0) }

                drop(initialize_on_drop);
            }
            Err(_) => {
                debug_assert!(self.initialized());
            }
        }
    }

    /// Forces the evaluation of this lazy value and returns a reference to the result.
    ///
    /// # Panics
    ///
    /// If the initialization function panics, the `Lazy` is poisoned
    /// and all subsequent calls to this function will panic.
    #[inline]
    pub async fn force_pin(self: Pin<&Self>) -> Pin<&T> {
        if !self.initialized() {
            self.initialize().await;
        }
        // SAFETY: we just initialized the `Lazy`
        (unsafe { self.get_unchecked_pin() })
            .unwrap_or_else(|| panic!("Lazy instance has previously been poisoned"))
    }

    /// Forces the evaluation of this lazy value and returns a mutable reference to the result.
    ///
    /// # Panics
    ///
    /// If the initialization function panics, the `Lazy` is poisoned
    /// and all subsequent calls to this function will panic.
    #[inline]
    pub async fn force_pin_mut(mut self: Pin<&mut Self>) -> Pin<&mut T> {
        if !self.as_mut().initialized_pin_mut() {
            // SAFETY: the cell is not initialized, so it stores `F` or `Fut`.
            // And `F` is `Unpin`. So making a pinned reference is safe.
            self.as_ref().initialize().await;
        }
        // SAFETY: we just initialized the `Lazy`
        (unsafe { self.get_unchecked_pin_mut() })
            .unwrap_or_else(|| panic!("Lazy instance has previously been poisoned"))
    }
}

impl<T, Fut, F> Drop for Lazy<T, Fut, F> {
    #[inline]
    fn drop(&mut self) {
        if self.initialized_mut() {
            // SAFETY: we just checked for the `Lazy` being initialized.
            // We are inside `drop`, so nobody will access our fields after us.
            unsafe { self.value.with_mut(|v| ManuallyDrop::drop(&mut (*v).init)) };
        } else {
            // SAFETY: we just check for the `Lazy` being uninitialized.
            // We hold an `&mut` to this `Lazy`, so nobody else is in the process of initializing it.
            // We are inside `drop`, so nobody will access our fields after us.
            unsafe {
                self.value.with_mut(|v| match &mut *(*v).uninit {
                    LazyUninitData::Function(f) => ManuallyDrop::drop(f),
                    LazyUninitData::Future(fut) => ManuallyDrop::drop(fut),
                })
            };
        }
    }
}

impl<T: fmt::Debug, Fut, F> fmt::Debug for Lazy<T, Fut, F> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get() {
            Some(v) => f.debug_tuple("Lazy").field(v).finish(),
            None => f.write_str("Lazy(Uninit)"),
        }
    }
}

unsafe impl<T: Send, Fut: Send, F: Send> Send for Lazy<T, Fut, F> {}

// We never create a `&F` from a `&Lazy<T, F>` so it is fine
// to not require a `Sync` bound on `F`.
// Need `T: Send + Sync` for `Sync` as the thread that initializes
// the cell could be different from the one that destroys it.
unsafe impl<T: Send + Sync, Fut: Send, F: Send> Sync for Lazy<T, Fut, F> {}

impl<T: UnwindSafe, Fut: UnwindSafe, F: UnwindSafe> UnwindSafe for Lazy<T, Fut, F> {}
impl<T: UnwindSafe + RefUnwindSafe, Fut: UnwindSafe, F: UnwindSafe> RefUnwindSafe
    for Lazy<T, F, Fut>
{
}

// F is not structurally pinned.
impl<T: Unpin, Fut: Unpin, F> Unpin for Lazy<T, Fut, F> {}
