use super::Semaphore;
use crate::loom::cell::UnsafeCell;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe cell which can be written to only once.
///
/// Provides the functionality to either set the value, in case `OnceCell`
/// is uninitialized, or get the already initialized value by using an async
/// function via [`OnceCell::get_or_init_with`] or by using a Future via
/// [`OnceCell::get_or_init`] directly via [`OnceCell::get_or_init`].
///
/// [`OnceCell::get_or_init_with`]: crate::sync::OnceCell::get_or_init_with
/// [`OnceCell::get_or_init`]: crate::sync::OnceCell::get_or_init
///
/// # Examples
/// ```
/// use tokio::sync::OnceCell;
///
/// async fn some_computation() -> u32 {
///     1 + 1
/// }
///
/// static ONCE: OnceCell<u32> = OnceCell::new();
///
/// #[tokio::main]
/// async fn main() {
///     let result1 = tokio::spawn(async {
///         ONCE.get_or_init_with(some_computation).await
///     }).await.unwrap();
///     assert_eq!(*result1, 2);
/// }
/// ```
pub struct OnceCell<T> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    semaphore: Semaphore,
}

impl<T> Default for OnceCell<T> {
    fn default() -> OnceCell<T> {
        OnceCell::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for OnceCell<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OnceCell")
            .field("value", &self.get())
            .finish()
    }
}

impl<T: Clone> Clone for OnceCell<T> {
    fn clone(&self) -> OnceCell<T> {
        let new_cell = OnceCell::new();
        if let Ok(value) = self.get() {
            match new_cell.set(value.clone()) {
                Ok(()) => (),
                Err(_) => unreachable!(),
            }
        }
        new_cell
    }
}

impl<T: PartialEq> PartialEq for OnceCell<T> {
    fn eq(&self, other: &OnceCell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for OnceCell<T> {}

impl<T> OnceCell<T> {
    /// Creates a new uninitialized OnceCell instance.
    #[cfg(all(feature = "parking_lot", not(all(loom, test)),))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    pub const fn new() -> Self {
        OnceCell {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            semaphore: Semaphore::const_new(1),
        }
    }

    /// Whether the value of the OnceCell is set or not.
    pub fn initialized(&self) -> bool {
        self.value_set.load(Ordering::Acquire)
    }

    // SAFETY: safe to call only once self.initialized() is true
    unsafe fn get_unchecked(&self) -> &T {
        &*self.value.with(|ptr| (*ptr).as_ptr())
    }

    // SAFETY: safe to call only once a permit on the semaphore has been
    // acquired
    unsafe fn set_value(&self, value: T) {
        self.value.with_mut(|ptr| (*ptr).as_mut_ptr().write(value));
        self.value_set.store(true, Ordering::Release);
        self.semaphore.close();
    }

    /// Tries to get a reference to the value of the OnceCell.
    ///
    /// Returns [`NotInitializedError`] if the value of the OnceCell
    /// hasn't previously been initialized.
    ///
    /// [`NotInitializedError`]: crate::sync::NotInitializedError
    pub fn get(&self) -> Result<&T, NotInitializedError> {
        if self.initialized() {
            Ok(unsafe { self.get_unchecked() })
        } else {
            Err(NotInitializedError)
        }
    }

    /// Sets the value of the OnceCell to the argument value.
    ///
    /// If the value of the OnceCell was already set prior to this call
    /// or some other set is currently initializing the cell, then
    /// [`AlreadyInitializedError`] is returned. In order to wait
    /// for an ongoing initialization to finish, call [`OnceCell::get_or_init`]
    /// or [`OnceCell::get_or_init_with`] instead.
    ///
    /// [`AlreadyInitializedError`]: crate::sync::AlreadyInitializedError
    /// ['OnceCell::get_or_init`]: crate::sync::OnceCell::get_or_init
    /// ['OnceCell::get_or_init_with`]: crate::sync::OnceCell::get_or_init_with
    pub fn set(&self, value: T) -> Result<(), AlreadyInitializedError> {
        if !self.initialized() {
            // After acquire().await we have either acquired a permit while self.value
            // is still uninitialized, or another thread has intialized the value and
            // closed the semaphore, in which case self.initialized is true and we
            // don't set the value
            match self.semaphore.try_acquire() {
                Ok(_permit) => {
                    if !self.initialized() {
                        // SAFETY: There is only one permit on the semaphore, hence only one
                        // mutable reference is created
                        unsafe { self.set_value(value) };

                        return Ok(());
                    } else {
                        unreachable!(
                            "acquired the permit after OnceCell value was already initialized."
                        );
                    }
                }
                _ => {
                    if !self.initialized() {
                        panic!(
                            "couldn't acquire a permit even though OnceCell value is uninitialized."
                        );
                    }
                }
            }
        }

        Err(AlreadyInitializedError)
    }

    /// Tries to initialize the value of the OnceCell using the async function `f`.
    /// If the value of the OnceCell was already initialized prior to this call,
    /// a reference to that initialized value is returned. If some other thread
    /// initiated the initialization prior to this call and the initialization
    /// hasn't completed, this call waits until the initialization is finished.
    ///
    /// This will deadlock if `f` tries to initialize the cell itself.
    pub async fn get_or_init_with<F, Fut>(&self, f: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        if self.initialized() {
            // SAFETY: once the value is initialized, no mutable references are given out, so
            // we can give out arbitrarily many immutable references
            unsafe { self.get_unchecked() }
        } else {
            // After acquire().await we have either acquired a permit while self.value
            // is still uninitialized, or current thread is awoken after another thread
            // has intialized the value and closed the semaphore, in which case self.initialized
            // is true and we don't set the value here
            match self.semaphore.acquire().await {
                Ok(_permit) => {
                    if !self.initialized() {
                        // If `f()` panics or `select!` is called, this `get_or_init_with` call
                        // is aborted and the semaphore permit is dropped.
                        let value = f().await;

                        // SAFETY: There is only one permit on the semaphore, hence only one
                        // mutable reference is created
                        unsafe { self.set_value(value) };

                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        unsafe { self.get_unchecked() }
                    } else {
                        unreachable!("acquired semaphore after value was already initialized.");
                    }
                }
                Err(_) => {
                    if self.initialized() {
                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        unsafe { self.get_unchecked() }
                    } else {
                        unreachable!(
                            "Semaphore closed, but the OnceCell has not been initialized."
                        );
                    }
                }
            }
        }
    }

    /// Tries to initialize the value of the `OnceCell` using the the Future `f`.
    /// If the value of the `OnceCell` was already initialized prior to this call,
    /// a reference to that initialized value is returned. If some other thread
    /// initiated the initialization prior to this call and the initialization
    /// hasn't completed, this call waits until the initialization is finished.
    ///
    /// This will deadlock if `f` internally tries to initialize the cell itself.
    pub async fn get_or_init<F>(&self, f: F) -> &T
    where
        F: Future<Output = T>,
    {
        if self.initialized() {
            // SAFETY: once the value is initialized, no mutable references are given out, so
            // we can give out arbitrarily many immutable references
            return unsafe { self.get_unchecked() };
        } else {
            // After acquire().await we have either acquired a permit while self.value
            // is still uninitialized, or current thread is awoken after another thread
            // has intialized the value and closed the semaphore, in which case self.initialized
            // is true and we don't set the value here
            match self.semaphore.acquire().await {
                Ok(_permit) => {
                    if !self.initialized() {
                        // If `f` panics or `select!` is called, this `get_or_init` call
                        // is aborted and the semaphore permit is dropped.
                        let value = f.await;

                        // SAFETY: There is only one permit on the semaphore, hence only one
                        // mutable reference is created
                        unsafe { self.set_value(value) };

                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        return unsafe { self.get_unchecked() };
                    } else {
                        unreachable!("acquired semaphore after value was already initialized.");
                    }
                }
                Err(_) => {
                    if self.initialized() {
                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        return unsafe { self.get_unchecked() };
                    } else {
                        unreachable!(
                            "Semaphore closed, but the OnceCell has not been initialized."
                        );
                    }
                }
            }
        }
    }
}

// Since `get` gives us access to immutable references of the
// OnceCell, OnceCell can only be Sync if T is Sync, otherwise
// OnceCell would allow sharing references of !Sync values across
// threads. We need T to be Send in order for OnceCell to by Sync
// because we can use `set` on `&OnceCell<T>` to send
// values (of type T) across threads.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}

// Access to OnceCell's value is guarded by the semaphore permit
// and atomic operations on `value_set`, so as long as T itself is Send
// it's safe to send it to another thread
unsafe impl<T: Send> Send for OnceCell<T> {}

/// Error returned from the [`OnceCell::set`] method
///
/// [`OnceCell::set`]: crate::sync::OnceCell::set
#[derive(Debug, PartialEq)]
pub struct AlreadyInitializedError;

impl fmt::Display for AlreadyInitializedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AlreadyInitializedError")
    }
}

impl Error for AlreadyInitializedError {}

/// Error returned from the [`OnceCell::get`] method
///
/// [`OnceCell::get`]: crate::sync::OnceCell::get
#[derive(Debug, PartialEq)]
pub struct NotInitializedError;

impl fmt::Display for NotInitializedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NotInitializedError")
    }
}

impl Error for NotInitializedError {}
