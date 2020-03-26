use std::fmt;
use std::ops::Deref;

/// `AtomicPtr` providing an additional `load_unsync` function.
pub(crate) struct AtomicPtr<T> {
    inner: std::sync::atomic::AtomicPtr<T>,
}

impl<T> AtomicPtr<T> {
    pub(crate) fn new(ptr: *mut T) -> AtomicPtr<T> {
        let inner = std::sync::atomic::AtomicPtr::new(ptr);
        AtomicPtr { inner }
    }

    pub(crate) fn with_mut<R>(&mut self, f: impl FnOnce(&mut *mut T) -> R) -> R {
        f(self.inner.get_mut())
    }
}

impl<T> Deref for AtomicPtr<T> {
    type Target = std::sync::atomic::AtomicPtr<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> fmt::Debug for AtomicPtr<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(fmt)
    }
}
