use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

/// A pointer type similar to `Box` that owns a value, but doesn't own the
/// storage location containing the value.
pub(crate) struct OwningPtr<'a, T> {
    storage: &'a mut ManuallyDrop<T>,
}

impl<'a, T> OwningPtr<'a, T> {
    /// Take ownership of the value in the `ManuallyDrop`.
    ///
    /// # Safety
    ///
    /// The storage must contain a valid value before this call, and it should be
    /// considered uninitialized after the `OwningPtr` is destroyed.
    pub(crate) unsafe fn new(storage: &'a mut ManuallyDrop<T>) -> Self {
        Self { storage }
    }

    /// Take ownership of the value.
    #[inline]
    pub(crate) fn into_inner(self) -> T {
        // Don't run destructor of `self`.
        let this = ManuallyDrop::new(self);

        // safety: The creator of the OwningPtr guarantees that the storage
        // contains a valid value, and promises to treat it as uninitialized
        // afterwards.
        unsafe {
            ManuallyDrop::take(this.storage)
        }
    }
}

impl<'a, T> Deref for OwningPtr<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &**self.storage
    }
}

impl<'a, T> DerefMut for OwningPtr<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut **self.storage
    }
}

impl<'a, T> Drop for OwningPtr<'a, T> {
    fn drop(&mut self) {
        // safety: The creator of the OwningPtr guarantees that the storage
        // contains a valid value, and promises to treat it as uninitialized
        // afterwards.
        unsafe {
            ManuallyDrop::drop(self.storage)
        }
    }
}
