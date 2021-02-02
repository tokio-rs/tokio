#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

use super::Semaphore;
use crate::loom::cell::UnsafeCell;
use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe cell which can be written to only once.
///
/// Provides the functionality to set the value by using an async
/// function via [`OnceCell::get_or_init`]
///
/// [`OnceCell::get_or_init`]: crate::sync::OnceCell::get_or_init
///
/// # Examples
/// ```
/// use tokio::sync::OnceCell;
/// use std::pin::Pin;
/// use std::future::Future;
///
/// async fn some_computation() -> u32 {
///     1 + 1
/// }
///
/// fn call_async_fn() -> Pin<Box<dyn Future<Output = u32> + Send>> {
///     Box::pin(some_computation())
/// }
///
/// #[tokio::main]
/// async fn main() {
///     static ONCE: OnceCell<u32> = OnceCell::new();
///     let result1 = tokio::spawn(async {
///         ONCE.get_or_init(call_async_fn).await
///     }).await.unwrap();
///     assert_eq!(*result1, 2);
/// }
pub struct OnceCell<T> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    sema: Semaphore,
}

impl<T> Default for OnceCell<T> {
    fn default() -> OnceCell<T> {
        OnceCell::new()
    }
}

impl<T> fmt::Debug for OnceCell<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OnceCell")
            .field("value", &self.value)
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
    pub const fn new() -> Self {
        OnceCell {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            sema: Semaphore::const_new(1),
        }
    }

    fn initialized(&self) -> bool {
        self.value_set.load(Ordering::Acquire)
    }

    fn set_to_initialized(&self) {
        self.value_set.store(true, Ordering::Release);
    }

    unsafe fn get_unchecked(&self) -> &T {
        &*self.value.with(|ptr| (*ptr).as_ptr())
    }

    unsafe fn set_value(&self, value: T) {
        self.value.with_mut(|ptr| (*ptr).as_mut_ptr().write(value));
    }

    /// Tries to get a reference to the value of the OnceCell.
    ///
    /// Returns [`OnceCellError::NotInitialized`] if the value of the OnceCell
    /// hasn't previously been initialized
    ///
    /// [`OnceCellError::NotInitialized`]: crate::sync::OnceCellError::NotInitialized
    pub fn get(&self) -> Result<&T, OnceCellError> {
        if self.initialized() {
            Ok(unsafe { self.get_unchecked() })
        } else {
            Err(OnceCellError::NotInitialized)
        }
    }

    /// Sets the value of the OnceCell to the argument value.
    ///
    /// If the value of the OnceCell was already set prior to this call, then
    /// [`OnceCellError::AlreadyInitialized`] is returned
    ///
    /// [`OnceCellError::AlreadyInitialized`]: crate::sync::OnceCellError::AlreadyInitialized
    pub fn set(&self, value: T) -> Result<(), OnceCellError> {
        if !self.initialized() {
            // After acquire().await we have either acquired a permit while self.value
            // is still uninitialized, or another thread has intialized the value and
            // closed the semaphore, in which case self.initialized is true and we
            // don't set the value
            match self.sema.try_acquire() {
                Ok(_permit) => {
                    if !self.initialized() {
                        // SAFETY: There is only one permit on the semaphore, hence only one
                        // mutable reference is created
                        unsafe { self.set_value(value) };

                        self.set_to_initialized();
                        self.sema.close();
                        return Ok(());
                    } else {
                        panic!("acquired the permit after OnceCell value was already initialized");
                    }
                }
                _ => {
                    if !self.initialized() {
                        panic!(
                            "couldn't acquire a permit even though OnceCell value is uninitialized"
                        );
                    }
                }
            }
        }

        Err(OnceCellError::AlreadyInitialized)
    }

    /// Tries to initialize the value of the OnceCell using the async function `f`.
    /// If the value of the OnceCell had already been initialized prior to this call
    /// a reference to that initialized value is returned
    ///
    /// This will deadlock if `f` tries to initialize the cell itself.
    pub async fn get_or_init<F>(&self, f: F) -> &T
    where
        F: Fn() -> Pin<Box<dyn Future<Output = T> + Send>>,
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
            match self.sema.acquire().await {
                Ok(_permit) => {
                    if !self.initialized() {
                        let value = f().await;

                        // SAFETY: There is only one permit on the semaphore, hence only one
                        // mutable reference is created
                        unsafe { self.set_value(value) };

                        // It's important that we close the semaphore only after set_to_initialized call
                        // otherwise a waiting thread would be woken up and find an uninitialized
                        // state after failing to acquire a permit, which causes a panic
                        self.set_to_initialized();
                        self.sema.close();

                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        return unsafe { self.get_unchecked() };
                    } else {
                        panic!("acquired semaphore after value was already initialized");
                    }
                }
                Err(_) => {
                    if self.initialized() {
                        // SAFETY: once the value is initialized, no mutable references are given out, so
                        // we can give out arbitrarily many immutable references
                        return unsafe { self.get_unchecked() };
                    } else {
                        panic!("semaphore acquire call failed when value not already intialized");
                    }
                }
            }
        }
    }
}

unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

/// Error returned from the [`OnceCell::get`] and [`OnceCell::set`] methods
///
/// [`OnceCell::get`]: crate::sync::OnceCell::get
/// [`OnceCell::set`]: crate::sync::OnceCell::set
#[derive(Debug, PartialEq)]
pub enum OnceCellError {
    /// The value in OnceCell was already initialized
    AlreadyInitialized,

    /// The value in OnceCell was not previously initialized
    NotInitialized,
}

/// Allows one to lazily call an async function
///
/// # Examples
/// ```
/// use tokio::sync::Lazy;
/// use std::pin::Pin;
/// use std::future::Future;
///
///
/// async fn some_computation() -> u32 {
///     1 + 1
/// }
///
/// fn call_async_fn() -> Pin<Box<dyn Future<Output = u32> + Send>> {
///     Box::pin(some_computation())
/// }
///
/// #[tokio::main]
/// async fn main() {
///     static LAZY : Lazy<u32> = Lazy::new(call_async_fn);
///
///     let result = tokio::spawn(async {
///         LAZY.get().await
///     }).await.unwrap();
///     assert_eq!(*result, 2);
/// }
/// ```
pub struct Lazy<T, F = fn() -> Pin<Box<dyn Future<Output = T> + Send>>> {
    cell: OnceCell<T>,
    init: Cell<Option<F>>,
}

impl<T, F> Lazy<T, F> {
    /// Creates a new lazy value with the given initializing
    /// async function.
    pub const fn new(init: F) -> Lazy<T, F> {
        Lazy {
            cell: OnceCell::new(),
            init: Cell::new(Some(init)),
        }
    }
}

impl<T, F: Fn() -> Pin<Box<dyn Future<Output = T> + Send>>> Lazy<T, F> {
    /// On first call runs the function stored in Lazy and stores its return value.
    /// Each subsequent call only returns the stored value.
    pub async fn get(&self) -> &T {
        let get_f = || match self.init.take() {
            Some(f) => f(),
            None => panic!(),
        };

        self.cell.get_or_init(get_f).await
    }
}

impl<T: fmt::Debug, F> fmt::Debug for Lazy<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lazy")
            .field("cell", &self.cell)
            .field("init", &"..")
            .finish()
    }
}

// We never create a `&F` from a `&Lazy<T, F>` so it is fine
// to not require a `Sync` bound on `F`
unsafe impl<T, F: Send> Sync for Lazy<T, F> where OnceCell<T>: Sync {}

unsafe impl<T, F: Send> Send for Lazy<T, F> where OnceCell<T>: Send {}
