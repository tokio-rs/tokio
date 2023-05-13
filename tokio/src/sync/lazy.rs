use super::Semaphore;
use crate::loom::cell::UnsafeCell;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::Drop;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

/// The inner data of a `Lazy`.
/// Is `function` when not yet initialized,
/// `value` with `Some` when initialized,
/// and `value` with `None` when poisoned.
union LazyData<T, F> {
    value: ManuallyDrop<Option<T>>,
    function: ManuallyDrop<F>,
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
pub struct Lazy<T, F = fn() -> Pin<Box<dyn Future<Output = T> + Send>>> {
    value_set: AtomicBool,
    value: UnsafeCell<LazyData<T, F>>,
    semaphore: Semaphore,
}

impl<T, F> Lazy<T, F> {
    /// Creates a new empty `Lazy` instance with the given initializing
    /// async function.
    #[must_use]
    #[inline]
    pub fn new(init: F) -> Lazy<T, F> {
        Lazy {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(LazyData {
                function: ManuallyDrop::new(init),
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
    pub const fn const_new(init: F) -> Lazy<T, F> {
        Lazy {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(LazyData {
                function: ManuallyDrop::new(init),
            }),
            semaphore: Semaphore::const_new(1),
        }
    }

    /// Returns `true` if this `Lazy` has been initialized, and `false` otherwise.
    fn initialized(&self) -> bool {
        self.value_set.load(Ordering::Acquire)
    }

    /// Returns `true` if this `Lazy` has been initialized, and `false` otherwise.
    /// Because it takes a mutable reference, it doesn't require an atomic operation.
    fn initialized_mut(&mut self) -> bool {
        *self.value_set.get_mut()
    }

    /// Returns a reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    unsafe fn get_unchecked(&self) -> Option<&T> {
        unsafe { self.value.with(|v| (*v).value.as_ref()) }
    }

    /// Returns a mutable reference to the value,
    /// or `None` if it has been poisoned.
    ///
    /// # Safety
    ///
    /// The `Lazy` must be initialized.
    unsafe fn get_unchecked_mut(&mut self) -> Option<&mut T> {
        #[allow(clippy::needless_borrow)]
        unsafe {
            self.value.with_mut(|v| (&mut *v).value.as_mut())
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

    /// Consumes this Lazy returning the stored value.
    /// Returns Ok(value) if Lazy is initialized and Err(f) otherwise.
    ///
    /// # Panics
    ///
    /// If this `Lazy` is poisoned because the initialization function panicked,
    /// calls to this function will panic.

    pub fn into_value(self) -> Result<T, F> {
        // Prevent double-drop
        let mut me = ManuallyDrop::new(self);

        if me.initialized_mut() {
            // SAFETY: we just checked that the `Lazy` is initialized.
            let maybe_val = unsafe { me.value.with_mut(|v| ManuallyDrop::take(&mut (*v).value)) };
            Ok(maybe_val.unwrap_or_else(|| panic!("Lazy instance has previously been poisoned")))
        } else {
            // SAFETY: we just checked that the `Lazy` is uninitialized.
            // We own the `Lazy`, so there are no outstading references
            // and it is not in the process of initialization.
            Err(unsafe {
                me.value
                    .with_mut(|v| ManuallyDrop::take(&mut (*v).function))
            })
        }
    }
}

impl<T, F, Fut> Lazy<T, F>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    /// Initializes the `Lazy`, if it has not yet been.
    /// Once this has returned, `LazyData` is guaranteed to be in the `value` state.
    #[cold]
    async fn initialize(&self) {
        // Here we try to acquire the semaphore permit. Holding the permit
        // will allow us to set the value of the Lazy, and prevents
        // other tasks from initializing the Lazy while we are holding
        // it.
        match self.semaphore.acquire().await {
            Ok(permit) => {
                debug_assert!(!self.initialized());

                // No matter what happens, nobody will ever get another try
                // at initializing the Lazy.
                permit.forget();

                // SAFETY: This lazy is uninitialized, so it still stores the initializing function.
                let f = unsafe {
                    self.value
                        .with_mut(|v| ManuallyDrop::take(&mut (*v).function))
                };

                // If the function panics, we have to set the `Lazy` to poisoned.
                // So all the initialization work happens on the `Drop` of this struct.
                struct InitializeOnDrop<'a, T, F> {
                    lazy: &'a Lazy<T, F>,
                    value: ManuallyDrop<Option<T>>,
                }

                impl<'a, T, F> Drop for InitializeOnDrop<'a, T, F> {
                    fn drop(&mut self) {
                        // Write the new value to the `Lazy`.
                        // SAFETY: we hold the only semaphore permit.
                        // We are inside `drop`, so nobody will access our fields after us.
                        unsafe {
                            self.lazy.value.with_mut(|v| {
                                (*v).value = ManuallyDrop::new(ManuallyDrop::take(&mut self.value))
                            });
                        }
                        // FIXME this can be unsychronized when we have a mutable borrow of the `Lazy`
                        self.lazy.value_set.store(true, Ordering::Release);
                        self.lazy.semaphore.close();
                    }
                }

                // Arm the initialize-on-drop-mechanism.
                let mut initialize_on_drop = InitializeOnDrop {
                    lazy: self,
                    value: ManuallyDrop::new(None),
                };

                // Run the initializing function to get the new value.
                // If this panics, `initialize_on_drop` is dropped with
                // `value = None` and poisions the `Lazy`.
                // Otherwise, it is dropped with the new value and initializes
                // the `Lazy` with it.
                initialize_on_drop.value = ManuallyDrop::new(Some(f().await));

                drop(initialize_on_drop)
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
    pub async fn force(&self) -> &T {
        if !self.initialized() {
            self.initialize().await;
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
            self.initialize().await;
        }
        // SAFETY: we just initialized the `Lazy`
        (unsafe { self.get_unchecked_mut() })
            .unwrap_or_else(|| panic!("Lazy instance has previously been poisoned"))
    }
}

impl<T, F> Drop for Lazy<T, F> {
    #[inline]
    fn drop(&mut self) {
        if self.initialized_mut() {
            // SAFETY: we just checked for the `Lazy` being initialized.
            // We are inside `drop`, so nobody will access our fields after us.
            unsafe { self.value.with_mut(|v| ManuallyDrop::drop(&mut (*v).value)) };
        } else {
            // SAFETY: we just check for the `Lazy` being uninitialized.
            // We hold an `&mut` to this `Lazy`, so nobody else is in the process of initializing it.
            // We are inside `drop`, so nobody will access our fields after us.
            unsafe {
                self.value
                    .with_mut(|v| ManuallyDrop::drop(&mut (*v).function))
            };
        }
    }
}

impl<T: fmt::Debug, F> fmt::Debug for Lazy<T, F> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get() {
            Some(v) => f.debug_tuple("Lazy").field(v).finish(),
            None => f.write_str("Lazy(Uninit)"),
        }
    }
}

unsafe impl<T: Send, F: Send> Send for Lazy<T, F> {}

// We never create a `&F` from a `&Lazy<T, F>` so it is fine
// to not require a `Sync` bound on `F`.
// Need `T: Send + Sync` for `Sync` as the thread that intitializes
// the cell could be different from the one that destroys it.
unsafe impl<T: Send + Sync, F: Send> Sync for Lazy<T, F> {}

impl<T: UnwindSafe, F: UnwindSafe> UnwindSafe for Lazy<T, F> {}
impl<T: UnwindSafe + RefUnwindSafe, F: UnwindSafe> RefUnwindSafe for Lazy<T, F> {}
