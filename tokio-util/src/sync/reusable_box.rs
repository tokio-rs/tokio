use std::future::Future;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::{fmt, panic};
use std::panic::AssertUnwindSafe;
use std::ptr::{self, NonNull};
use std::alloc::{self, alloc, dealloc, Layout};

/// A resuable `Pin<Box<dyn Future<Output = T> + Send>>`.
///
/// This type lets you replace the future stored in the box without
/// reallocating when the size and alignment permits this.
pub struct ReusableBoxFuture<T> {
    boxed: NonNull<dyn Future<Output = T> + Send>,
    layout: Layout,
}

impl<T> ReusableBoxFuture<T> {
    /// Create a new `ReusableBoxFuture<T>` containing the provided future.
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        let layout = Layout::for_value(&future);
        let boxed: *mut F = unsafe { alloc(layout) as *mut F };

        match NonNull::new(boxed) {
            Some(boxed) => {
                unsafe {
                    ptr::write(boxed.as_ptr(), future);
                }

                Self {
                    boxed,
                    layout,
                }
            },
            None => {
                alloc::handle_alloc_error(layout);
            },
        }
    }

    /// Replace the future currently stored in this box.
    ///
    /// This reallocates if and only if the layout of the provided future is
    /// different from the layout of the currently stored future.
    pub fn set<F>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let layout = Layout::for_value(&future);

        if layout == self.layout {
            // SAFETY: We just checked that the layout of F is correct.
            unsafe {
                self.set_same_layout(future);
            }
        } else {
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
        F: Future<Output = T> + Send + 'static,
    {
        let layout = Layout::for_value(&future);

        if layout == self.layout {
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
        F: Future<Output = T> + Send + 'static,
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
            Ok(()) => {},
            Err(payload) => {
                panic::resume_unwind(payload);
            },
        }
    }

    /// Get a pinned reference to the underlying future.
    pub fn get_pin(&mut self) -> Pin<&mut (dyn Future<Output = T> + Send)> {
        unsafe {
            Pin::new_unchecked(self.boxed.as_mut())
        }
    }

    /// Poll the future stored inside this box.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        self.get_pin().poll(cx)
    }
}

impl<T> Future for ReusableBoxFuture<T> {
    type Output = T;

    /// Poll the future stored inside this box.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        Pin::into_inner(self).get_pin().poll(cx)
    }
}

// The future stored inside ReusableBoxFuture<T> must be Send.
unsafe impl<T> Send for ReusableBoxFuture<T> {}

// The only method called on self.boxed is poll, which takes &mut self, so this
// struct being Sync does not permit any invalid access to the Future, even if
// the future is not Sync.
unsafe impl<T> Sync for ReusableBoxFuture<T> {}

// Just like a Pin<Box<dyn Future>> is always Unpin, so is this type.
impl<T> Unpin for ReusableBoxFuture<T> {}

impl<T> Drop for ReusableBoxFuture<T> {
    fn drop(&mut self) {
        struct DeallocateOnDrop {
            ptr: *mut u8,
            layout: Layout,
        }
        impl Drop for DeallocateOnDrop {
            fn drop(&mut self) {
                unsafe {
                    dealloc(self.ptr, self.layout);
                }
            }
        }

        // This object guarantees that the memory is deallocated even if the
        // destructor panics.
        let dealloc_on_drop = DeallocateOnDrop {
            ptr: self.boxed.as_ptr() as *mut u8,
            layout: self.layout,
        };

        unsafe {
            ptr::drop_in_place(self.boxed.as_ptr());
        }

        // Explicitly call the destructor. Note that this also happens if
        // drop_in_place panics.
        drop(dealloc_on_drop);
    }
}

impl<T> fmt::Debug for ReusableBoxFuture<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReusableBoxFuture").finish()
    }
}
