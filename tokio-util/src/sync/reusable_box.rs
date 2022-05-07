use std::alloc::Layout;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::task::{Context, Poll};
use std::{fmt, panic};

/// A reusable `Pin<Box<dyn Future<Output = T> + Send + 'a>>`.
///
/// This type lets you replace the future stored in the box without
/// reallocating when the size and alignment permits this.
pub struct ReusableBoxFuture<'a, T> {
    boxed: NonNull<dyn Future<Output = T> + Send + 'a>,
}

impl<'a, T> ReusableBoxFuture<'a, T> {
    /// Create a new `ReusableBoxFuture<T>` containing the provided future.
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'a,
    {
        let boxed: Box<dyn Future<Output = T> + Send + 'a> = Box::new(future);

        let boxed = NonNull::from(Box::leak(boxed));

        Self { boxed }
    }

    /// Replace the future currently stored in this box.
    ///
    /// This reallocates if and only if the layout of the provided future is
    /// different from the layout of the currently stored future.
    pub fn set<F>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'a,
    {
        if let Err(future) = self.try_set(future) {
            *self = Self::new(future);
        }
    }

    /// Replace the future currently stored in this box.
    ///
    /// This function never reallocates, but returns an error if the provided
    /// future has a different size or alignment from the currently stored
    /// future.
    pub fn try_set<F>(&mut self, future: F) -> Result<(), F>
    where
        F: Future<Output = T> + Send + 'a,
    {
        // SAFETY: The pointer is not dangling.
        let self_layout = {
            let dyn_future: &(dyn Future<Output = T> + Send) = unsafe { self.boxed.as_ref() };
            Layout::for_value(dyn_future)
        };

        if Layout::new::<F>() == self_layout {
            // SAFETY: We just checked that the layout of F is correct.
            unsafe {
                self.set_same_layout(future);
            }

            Ok(())
        } else {
            Err(future)
        }
    }

    /// Set the current future.
    ///
    /// # Safety
    ///
    /// This function requires that the layout of the provided future is the
    /// same as `self.layout`.
    unsafe fn set_same_layout<F>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'a,
    {
        // Drop the existing future, catching any panics.
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            ptr::drop_in_place(self.boxed.as_ptr());
        }));

        // Overwrite the future behind the pointer. This is safe because the
        // allocation was allocated with the same size and alignment as the type F.
        let self_ptr: *mut F = self.boxed.as_ptr() as *mut F;
        ptr::write(self_ptr, future);

        // Update the vtable of self.boxed. The pointer is not null because we
        // just got it from self.boxed, which is not null.
        self.boxed = NonNull::new_unchecked(self_ptr);

        // If the old future's destructor panicked, resume unwinding.
        match result {
            Ok(()) => {}
            Err(payload) => {
                panic::resume_unwind(payload);
            }
        }
    }

    /// Get a pinned reference to the underlying future.
    pub fn get_pin(&mut self) -> Pin<&mut (dyn Future<Output = T> + Send)> {
        // SAFETY: The user of this box cannot move the box, and we do not move it
        // either.
        unsafe { Pin::new_unchecked(self.boxed.as_mut()) }
    }

    /// Poll the future stored inside this box.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        self.get_pin().poll(cx)
    }
}

impl<T> Future for ReusableBoxFuture<'_, T> {
    type Output = T;

    /// Poll the future stored inside this box.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        Pin::into_inner(self).get_pin().poll(cx)
    }
}

// The future stored inside ReusableBoxFuture<'_, T> must be Send since the
// `new` and `set` and `try_set` methods only allow setting the future to Send
// futures.
//
// Note that T is the return type of the future, so its not relevant for
// whether the future itself is Send.
unsafe impl<T> Send for ReusableBoxFuture<'_, T> {}

// The only method called on self.boxed is poll, which takes &mut self, so this
// struct being Sync does not permit any invalid access to the Future, even if
// the future is not Sync.
unsafe impl<T> Sync for ReusableBoxFuture<'_, T> {}

// Just like a Pin<Box<dyn Future>> is always Unpin, so is this type.
impl<T> Unpin for ReusableBoxFuture<'_, T> {}

impl<T> Drop for ReusableBoxFuture<'_, T> {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.boxed.as_ptr()));
        }
    }
}

impl<T> fmt::Debug for ReusableBoxFuture<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReusableBoxFuture").finish()
    }
}
