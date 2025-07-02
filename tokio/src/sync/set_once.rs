use super::{Notify, SetError};
use crate::{loom::cell::UnsafeCell, pin};
use std::fmt;
use std::mem::MaybeUninit;
use std::ops::Drop;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// This file contains an implementation of an SetOnce. The value of SetOnce
// can only be modified once during initialization.
//
//  1. When `value_set` is false, the `value` is not initialized and wait()
//      future will keep on waiting
//  2. When `value_set` is true, the wait() future completes, get() will return
//      Some(&T)
//
// The value cannot be changed after set() is called. Subsequent calls to set()
// will return a `SetError`

/// A thread-safe cell that can be written to only once.
/// A `SetOnce` is inspired from python's
/// [`asyncio.Event`](https://docs.python.org/3/library/asyncio-sync.html#asyncio.Event)
/// type. It can be used to wait until the value of the `SetOnce` is set like
/// a "Event" mechanism
///
/// # Example
///
/// ```
/// use tokio::sync::SetOnce;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let once = SetOnce::new();
///
///     let arc = Arc::new(once);
///     let first_cl = Arc::clone(&arc);
///     let second_cl = Arc::clone(&arc);
///
///     tokio::spawn(async move { first_cl.set(20) });
///
///     tokio::spawn(async move { second_cl.set(10) });
///
///     let res = arc.get(); // returns None
///     arc.wait().await; // lets wait until the value is set
///
///     println!("{:?}", arc.get());
/// }
/// ```
///
/// A `SetOnce` is typically used for global variables that need to be
/// initialized once on first use, but need no further changes. The `SetOnce`
/// in Tokio allows the initialization procedure to be asynchronous.
///
/// # Example
///
/// ```
/// use tokio::sync::{SetOnce, SetError};
///
///
/// static ONCE: SetOnce<u32> = SetOnce::const_new();
///
/// #[tokio::main]
/// async fn main() -> Result<(), SetError<u32>> {
///     ONCE.set(2)?;
///     let result = ONCE.get();
///     assert_eq!(result, Some(&2));
///
///     Ok(())
/// }
/// ```
pub struct SetOnce<T> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    notify: Notify,
    // we lock the mutex inside set to ensure
    // only one caller of set can run at a time
    lock: Mutex<()>,
}

impl<T> Default for SetOnce<T> {
    fn default() -> SetOnce<T> {
        SetOnce::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for SetOnce<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("SetOnce")
            .field("value", &self.get())
            .finish()
    }
}

impl<T: Clone> Clone for SetOnce<T> {
    fn clone(&self) -> SetOnce<T> {
        SetOnce::new_with(self.get().cloned())
    }
}

impl<T: PartialEq> PartialEq for SetOnce<T> {
    fn eq(&self, other: &SetOnce<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for SetOnce<T> {}

impl<T> Drop for SetOnce<T> {
    fn drop(&mut self) {
        if *self.value_set.get_mut() {
            // SAFETY: We're inside the drop implementation of SetOnce
            // AND we're also initalized. This is the best way to ensure
            // out data gets dropped
            unsafe { self.value.with_mut(|ptr| ptr::drop_in_place(ptr as *mut T)) }
            // no need to set the flag to false as this set once is being
            // dropped
        }
    }
}

impl<T> From<T> for SetOnce<T> {
    fn from(value: T) -> Self {
        SetOnce {
            value_set: AtomicBool::new(true),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            notify: Notify::new(),
            lock: Mutex::new(()),
        }
    }
}

impl<T> SetOnce<T> {
    /// Creates a new empty `SetOnce` instance.
    pub fn new() -> Self {
        Self {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            notify: Notify::new(),
            lock: Mutex::new(()),
        }
    }

    /// Creates a new empty `SetOnce` instance.
    ///
    /// Equivalent to `SetOnce::new`, except that it can be used in static
    /// variables.
    ///
    /// When using the `tracing` [unstable feature], a `SetOnce` created with
    /// `const_new` will not be instrumented. As such, it will not be visible
    /// in [`tokio-console`]. Instead, [`SetOnce::new`] should be used to
    /// create an instrumented object if that is needed.
    ///
    /// # Example
    ///
    /// ```
    /// use tokio::sync::{SetOnce, SetError};
    ///
    /// static ONCE: SetOnce<u32> = SetOnce::const_new();
    ///
    /// fn get_global_integer() -> Result<Option<&'static u32>, SetError<u32>> {
    ///     ONCE.set(2)?;
    ///     Ok(ONCE.get())
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), SetError<u32>> {
    ///     let result = get_global_integer()?;
    ///
    ///     assert_eq!(result, Some(&2));
    ///     Ok(())
    /// }
    /// ```
    ///
    /// [`tokio-console`]: https://github.com/tokio-rs/console
    /// [unstable feature]: crate#unstable-features
    #[cfg(not(all(loom, test)))]
    pub const fn const_new() -> Self {
        Self {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            notify: Notify::const_new(),
            lock: Mutex::new(()),
        }
    }

    /// Creates a new `SetOnce` that contains the provided value, if any.
    ///
    /// If the `Option` is `None`, this is equivalent to `SetOnce::new`.
    ///
    /// [`SetOnce::new`]: crate::sync::SetOnce::new
    pub fn new_with(value: Option<T>) -> Self {
        if let Some(v) = value {
            SetOnce::from(v)
        } else {
            SetOnce::new()
        }
    }

    /// Creates a new `SetOnce` that contains the provided value.
    ///
    /// # Example
    ///
    /// When using the `tracing` [unstable feature], a `SetOnce` created with
    /// `const_new_with` will not be instrumented. As such, it will not be
    /// visible in [`tokio-console`]. Instead, [`SetOnce::new_with`] should be
    /// used to create an instrumented object if that is needed.
    ///
    /// ```
    /// use tokio::sync::SetOnce;
    ///
    /// static ONCE: SetOnce<u32> = SetOnce::const_new_with(1);
    ///
    /// fn get_global_integer() -> Option<&'static u32> {
    ///     ONCE.get()
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = get_global_integer();
    ///
    ///     assert_eq!(result, Some(&1));
    /// }
    /// ```
    ///
    /// [`tokio-console`]: https://github.com/tokio-rs/console
    /// [unstable feature]: crate#unstable-features
    #[cfg(not(all(loom, test)))]
    pub const fn const_new_with(value: T) -> Self {
        Self {
            value_set: AtomicBool::new(true),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            notify: Notify::const_new(),
            lock: Mutex::new(()),
        }
    }

    /// Returns `true` if the `SetOnce` currently contains a value, and `false`
    /// otherwise.
    pub fn initialized(&self) -> bool {
        // Using acquire ordering so we're able to read/catch any writes that
        // are done with `Ordering::Release`
        self.value_set.load(Ordering::Acquire)
    }

    // SAFETY: The SetOnce must not be empty.
    unsafe fn get_unchecked(&self) -> &T {
        &*self.value.with(|ptr| (*ptr).as_ptr())
    }

    /// Returns a reference to the value currently stored in the `SetOnce`, or
    /// `None` if the `SetOnce` is empty.
    pub fn get(&self) -> Option<&T> {
        if self.initialized() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    // SAFETY: The caller of this function needs to ensure that this function is
    // called only when the value_set AtomicBool is flipped from FALSE to TRUE
    // meaning that the value is being set from uinitialized to initialized via
    // this function
    //
    // The caller also has to ensure writes on `value` are syncronized with a
    // external lock to prevent mutliple set_value calls at the same time.
    unsafe fn set_value(&self, value: T) {
        unsafe {
            self.value.with_mut(|ptr| (*ptr).as_mut_ptr().write(value));
        }

        // notify the waiting wakers that the value is set
        self.notify.notify_waiters();
    }

    /// Sets the value of the `SetOnce` to the given value if the `SetOnce` is
    /// empty.
    ///
    /// If the `SetOnce` already has a value, this call will fail with an
    /// [`SetError::AlreadyInitializedError`].
    ///
    /// If the `SetOnce` is empty, but some other task is currently trying to
    /// set the value, this call will fail with [`SetError::InitializingError`].
    ///
    /// [`SetError::AlreadyInitializedError`]: crate::sync::SetError::AlreadyInitializedError
    /// [`SetError::InitializingError`]: crate::sync::SetError::InitializingError
    pub fn set(&self, value: T) -> Result<(), SetError<T>> {
        if self.initialized() {
            return Err(SetError::AlreadyInitializedError(value));
        }

        // SAFETY: lock the mutex to ensure only one caller of set
        // can run at a time.
        match self.lock.lock() {
            Ok(_) => {
                // Using release ordering so any threads that read a true from this
                // atomic is able to read the value we just stored.
                if !self.value_set.swap(true, Ordering::Release) {
                    // SAFETY: We are swapping the value_set AtomicBool from FALSE to
                    // TRUE with it being previously false and followed by that we are
                    // initializing the unsafe Cell field with the value
                    unsafe {
                        self.set_value(value);
                    }

                    Ok(())
                } else {
                    Err(SetError::AlreadyInitializedError(value))
                }
            }
            Err(_) => {
                // If we failed to lock the mutex, it means some other task is
                // trying to set the value, so we return an error.
                Err(SetError::InitializingError(value))
            }
        }
    }

    /// Takes the value from the cell, destroying the cell in the process.
    /// Returns `None` if the cell is empty.
    pub fn into_inner(mut self) -> Option<T> {
        if self.initialized() {
            // Since we have taken ownership of self, its drop implementation
            // will be called by the end of this function, to prevent a double
            // free we will set the value_set to false so that the drop
            // implementation does not try to drop the value again.
            *self.value_set.get_mut() = false;

            // SAFETY: The SetOnce is currently initialized, we can assume the
            // value is initialized and return that, when we return the value
            // we give the drop handler to the return scope.
            Some(unsafe { self.value.with_mut(|ptr| ptr::read(ptr).assume_init()) })
        } else {
            None
        }
    }

    /// Waits until set is called. The future returned will keep blocking until
    /// the `SetOnce` is initialized.
    ///
    /// If the `SetOnce` is already initialized, it will return the value
    // immediately.
    ///
    /// # Panics
    ///
    /// If the `SetOnce` is not initialized after waiting, it will panic. To
    /// avoid this, use `get_wait()` which returns an `Option<&T>` instead of
    /// `&T`.
    pub async fn wait(&self) -> &T {
        match self.get_wait().await {
            Some(val) => val,
            _ => panic!("SetOnce::wait called but the SetOnce is not initialized"),
        }
    }

    /// Waits until set is called.
    ///
    /// If the state failed to initalize it will return `None`.
    pub async fn get_wait(&self) -> Option<&T> {
        let notify_fut = self.notify.notified();
        pin!(notify_fut);

        if self.value_set.load(Ordering::Acquire) {
            // SAFETY: the state is initialized
            return Some(unsafe { self.get_unchecked() });
        }
        // wait until the value is set
        (&mut notify_fut).await;

        // look at the state again
        if self.value_set.load(Ordering::Acquire) {
            // SAFETY: the state is initialized
            return Some(unsafe { self.get_unchecked() });
        }

        None
    }
}

// Since `get` gives us access to immutable references of the SetOnce, SetOnce
// can only be Sync if T is Sync, otherwise SetOnce would allow sharing
// references of !Sync values across threads. We need T to be Send in order for
// SetOnce to by Sync because we can use `set` on `&SetOnce<T>` to send values
// (of type T) across threads.
unsafe impl<T: Sync + Send> Sync for SetOnce<T> {}

// Access to SetOnce's value is guarded by the Atomic boolean flag
// and atomic operations on `value_set`, so as long as T itself is Send
// it's safe to send it to another thread
unsafe impl<T: Send> Send for SetOnce<T> {}
