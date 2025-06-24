use super::{Notify, SetError};
use crate::loom::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::ops::Drop;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

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
///     
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
/// # Examples
///
/// ```
/// use tokio::sync::SetOnce;
///
///
/// static ONCE: SetOnce<u32> = SetOnce::const_new();
///
/// #[tokio::main]
/// async fn main() {
///     let result = ONCE.set(2).await;
///     assert_eq!(*result, 2);
/// }
/// ```
///
/// It is often useful to write a wrapper method for accessing the value.
///
/// ```
/// use tokio::sync::SetOnce;
///
/// static ONCE: SetOnce<u32> = SetOnce::const_new();
///
/// fn get_global_integer() -> &'static u32 {
///    ONCE.set(2);
///    ONCE.get().unwrap()
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let result = get_global_integer();
///     
///     assert_eq!(*result, 2);
/// }
/// ```
pub struct SetOnce<T> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    notify: Notify,
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
        if self.initialized() {
            unsafe {
                let _ = self.value.with_mut(|ptr| ptr::read(ptr).assume_init());
            }

            *self.value_set.get_mut() = false;
        }
    }
}

impl<T> From<T> for SetOnce<T> {
    fn from(value: T) -> Self {
        SetOnce {
            value_set: AtomicBool::new(true),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            notify: Notify::new(),
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
    /// use tokio::sync::SetOnce;
    ///
    /// static ONCE: SetOnce<u32> = SetOnce::const_new();
    ///
    /// fn get_global_integer() -> &'static u32 {
    ///     ONCE.set(2);
    ///     ONCE.get().unwrap();
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = get_global_integer();
    ///
    ///     assert_eq!(*result, 2);
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
    /// fn get_global_integer() -> &'static u32 {
    ///     ONCE.get().unwrap();
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = get_global_integer();
    ///     assert_eq!(*result, 1);
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
        }
    }

    /// Returns `true` if the `SetOnce` currently contains a value, and `false`
    /// otherwise.
    pub fn initialized(&self) -> bool {
        // Using acquire ordering so any threads that read a true from this
        // atomic is able to read the value.
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
            Err(SetError::InitializingError(value))
        }
    }

    /// Takes the value from the cell, destroying the cell in the process.
    /// Returns `None` if the cell is empty.
    pub fn into_inner(mut self) -> Option<T> {
        if self.initialized() {
            // SAFETY: The SetOnce is initialized, we can assume the value is
            // initialized and return that, when we return the value we give the
            // drop handler to someone else and drop is called automatically
            //
            // We set the value_set as value as we just took the value out
            // and the drop implementation will do nothing.
            *self.value_set.get_mut() = false;
            Some(unsafe { self.value.with_mut(|ptr| ptr::read(ptr).assume_init()) })
        } else {
            None
        }
        // the drop of self is called here again but it doesn't do anything since
        // if we dont return early then value is not initalized.
    }

    /// Takes ownership of the current value, leaving the cell empty.  Returns
    /// `None` if the cell is empty.
    pub fn take(&mut self) -> Option<T> {
        std::mem::take(self).into_inner()
    }

    /// Waits until the `SetOnce` has been initialized. Once the `SetOnce` is
    /// initialized the wakers are notified and the Future returned from this
    /// function completes.
    ///
    /// If this function is called after the `SetOnce` is initalized then
    /// empty future is returned which completes immediately.
    pub async fn wait(&self) {
        if !self.initialized() {
            let _ = self.notify.notified().await;
        }
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
