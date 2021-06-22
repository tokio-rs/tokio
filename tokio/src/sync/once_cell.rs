use super::Semaphore;
use crate::loom::cell::UnsafeCell;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::mem::MaybeUninit;
use std::ops::Drop;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe cell which can be written to only once.
///
/// Provides the functionality to either set the value, in case `OnceCell`
/// is uninitialized, or get the already initialized value by using an async
/// function via [`OnceCell::get_or_init`].
///
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
/// static ONCE: OnceCell<u32> = OnceCell::const_new();
///
/// #[tokio::main]
/// async fn main() {
///     let result1 = ONCE.get_or_init(some_computation).await;
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
        OnceCell::new_with(self.get().cloned())
    }
}

impl<T: PartialEq> PartialEq for OnceCell<T> {
    fn eq(&self, other: &OnceCell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for OnceCell<T> {}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        if self.initialized() {
            unsafe {
                self.value
                    .with_mut(|ptr| ptr::drop_in_place((&mut *ptr).as_mut_ptr()));
            };
        }
    }
}

impl<T> From<T> for OnceCell<T> {
    fn from(value: T) -> Self {
        let semaphore = Semaphore::new(0);
        semaphore.close();
        OnceCell {
            value_set: AtomicBool::new(true),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            semaphore,
        }
    }
}

impl<T> OnceCell<T> {
    /// Creates a new uninitialized OnceCell instance.
    pub fn new() -> Self {
        OnceCell {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            semaphore: Semaphore::new(1),
        }
    }

    /// Creates a new initialized OnceCell instance if `value` is `Some`, otherwise
    /// has the same functionality as [`OnceCell::new`].
    ///
    /// [`OnceCell::new`]: crate::sync::OnceCell::new
    pub fn new_with(value: Option<T>) -> Self {
        if let Some(v) = value {
            OnceCell::from(v)
        } else {
            OnceCell::new()
        }
    }

    /// Creates a new uninitialized OnceCell instance.
    #[cfg(all(feature = "parking_lot", not(all(loom, test)),))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    pub const fn const_new() -> Self {
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

    // SAFETY: safe to call only once self.initialized() is true. Safe because
    // because of the mutable reference.
    unsafe fn get_unchecked_mut(&mut self) -> &mut T {
        &mut *self.value.with_mut(|ptr| (*ptr).as_mut_ptr())
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
    /// Returns None if the value of the OnceCell hasn't previously been initialized.
    pub fn get(&self) -> Option<&T> {
        if self.initialized() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    /// Tries to return a mutable reference to the value of the cell.
    ///
    /// Returns None if the cell hasn't previously been initialized.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.initialized() {
            Some(unsafe { self.get_unchecked_mut() })
        } else {
            None
        }
    }

    /// Sets the value of the OnceCell to the argument value.
    ///
    /// If the value of the OnceCell was already set prior to this call
    /// then [`SetError::AlreadyInitializedError`] is returned. If another thread
    /// is initializing the cell while this method is called,
    /// [`SetError::InitializingError`] is returned. In order to wait
    /// for an ongoing initialization to finish, call
    /// [`OnceCell::get_or_init`] instead.
    ///
    /// [`SetError::AlreadyInitializedError`]: crate::sync::SetError::AlreadyInitializedError
    /// [`SetError::InitializingError`]: crate::sync::SetError::InitializingError
    /// ['OnceCell::get_or_init`]: crate::sync::OnceCell::get_or_init
    pub fn set(&self, value: T) -> Result<(), SetError<T>> {
        if !self.initialized() {
            // Another thread might be initializing the cell, in which case `try_acquire` will
            // return an error
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
                    // Couldn't acquire the permit, look if initializing process is already completed
                    if !self.initialized() {
                        return Err(SetError::InitializingError(value));
                    }
                }
            }
        }

        Err(SetError::AlreadyInitializedError(value))
    }

    /// Tries to initialize the value of the OnceCell using the async function `f`.
    /// If the value of the OnceCell was already initialized prior to this call,
    /// a reference to that initialized value is returned. If some other thread
    /// initiated the initialization prior to this call and the initialization
    /// hasn't completed, this call waits until the initialization is finished.
    ///
    /// This will deadlock if `f` tries to initialize the cell itself.
    pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
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
            // is still uninitialized, or the current thread is awoken after another thread
            // has intialized the value and closed the semaphore, in which case self.initialized
            // is true and we don't set the value here
            match self.semaphore.acquire().await {
                Ok(_permit) => {
                    if !self.initialized() {
                        // If `f()` panics or `select!` is called, this `get_or_init` call
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

    /// Tries to initialize the value of the OnceCell using the async function `f`.
    /// If the value of the OnceCell was already initialized prior to this call,
    /// a reference to that initialized value is returned. If some other thread
    /// initiated the initialization prior to this call and the initialization
    /// hasn't completed, this call waits until the initialization is finished.
    /// If the function argument `f` returns an error, `get_or_try_init`
    /// returns that error, otherwise the result of `f` will be stored in the cell.
    ///
    /// This will deadlock if `f` tries to initialize the cell itself.
    pub async fn get_or_try_init<E, F, Fut>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if self.initialized() {
            // SAFETY: once the value is initialized, no mutable references are given out, so
            // we can give out arbitrarily many immutable references
            unsafe { Ok(self.get_unchecked()) }
        } else {
            // After acquire().await we have either acquired a permit while self.value
            // is still uninitialized, or the current thread is awoken after another thread
            // has intialized the value and closed the semaphore, in which case self.initialized
            // is true and we don't set the value here
            match self.semaphore.acquire().await {
                Ok(_permit) => {
                    if !self.initialized() {
                        // If `f()` panics or `select!` is called, this `get_or_try_init` call
                        // is aborted and the semaphore permit is dropped.
                        let value = f().await;

                        match value {
                            Ok(value) => {
                                // SAFETY: There is only one permit on the semaphore, hence only one
                                // mutable reference is created
                                unsafe { self.set_value(value) };

                                // SAFETY: once the value is initialized, no mutable references are given out, so
                                // we can give out arbitrarily many immutable references
                                unsafe { Ok(self.get_unchecked()) }
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        unreachable!("acquired semaphore after value was already initialized.");
                    }
                }
                Err(_) => {
                    if self.initialized() {
                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        unsafe { Ok(self.get_unchecked()) }
                    } else {
                        unreachable!(
                            "Semaphore closed, but the OnceCell has not been initialized."
                        );
                    }
                }
            }
        }
    }

    /// Moves the value out of the cell, destroying the cell in the process.
    ///
    /// Returns `None` if the cell is uninitialized.
    pub fn into_inner(mut self) -> Option<T> {
        if self.initialized() {
            // Set to uninitialized for the destructor of `OnceCell` to work properly
            *self.value_set.get_mut() = false;
            Some(unsafe { self.value.with(|ptr| ptr::read(ptr).assume_init()) })
        } else {
            None
        }
    }

    /// Takes ownership of the current value, leaving the cell uninitialized.
    ///
    /// Returns `None` if the cell is uninitialized.
    pub fn take(&mut self) -> Option<T> {
        std::mem::take(self).into_inner()
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

/// Errors that can be returned from [`OnceCell::set`]
///
/// [`OnceCell::set`]: crate::sync::OnceCell::set
#[derive(Debug, PartialEq)]
pub enum SetError<T> {
    /// Error resulting from [`OnceCell::set`] calls if the cell was previously initialized.
    ///
    /// [`OnceCell::set`]: crate::sync::OnceCell::set
    AlreadyInitializedError(T),

    /// Error resulting from [`OnceCell::set`] calls when the cell is currently being
    /// inintialized during the calls to that method.
    ///
    /// [`OnceCell::set`]: crate::sync::OnceCell::set
    InitializingError(T),
}

impl<T> fmt::Display for SetError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::AlreadyInitializedError(_) => write!(f, "AlreadyInitializedError"),
            SetError::InitializingError(_) => write!(f, "InitializingError"),
        }
    }
}

impl<T: fmt::Debug> Error for SetError<T> {}

impl<T> SetError<T> {
    /// Whether `SetError` is `SetError::AlreadyInitializedError`.
    pub fn is_already_init_err(&self) -> bool {
        match self {
            SetError::AlreadyInitializedError(_) => true,
            SetError::InitializingError(_) => false,
        }
    }

    /// Whether `SetError` is `SetError::InitializingError`
    pub fn is_initializing_err(&self) -> bool {
        match self {
            SetError::AlreadyInitializedError(_) => false,
            SetError::InitializingError(_) => true,
        }
    }
}
