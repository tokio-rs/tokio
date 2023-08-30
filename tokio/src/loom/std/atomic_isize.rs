use std::cell::UnsafeCell;
use std::fmt;
use std::ops;

/// `AtomicIsize` providing an additional `unsync_load` function.
pub(crate) struct AtomicIsize {
    inner: UnsafeCell<std::sync::atomic::AtomicIsize>,
}

unsafe impl Send for AtomicIsize {}
unsafe impl Sync for AtomicIsize {}

impl AtomicIsize {
    pub(crate) const fn new(val: isize) -> AtomicIsize {
        let inner = UnsafeCell::new(std::sync::atomic::AtomicIsize::new(val));
        AtomicIsize { inner }
    }

    /// Performs an unsynchronized load.
    ///
    /// # Safety
    ///
    /// All mutations must have happened before the unsynchronized load.
    /// Additionally, there must be no concurrent mutations.
    pub(crate) unsafe fn unsync_load(&self) -> isize {
        core::ptr::read(self.inner.get() as *const isize)
    }

    pub(crate) fn with_mut<R>(&mut self, f: impl FnOnce(&mut isize) -> R) -> R {
        // safety: we have mutable access
        f(unsafe { (*self.inner.get()).get_mut() })
    }
}

impl ops::Deref for AtomicIsize {
    type Target = std::sync::atomic::AtomicIsize;

    fn deref(&self) -> &Self::Target {
        // safety: it is always safe to access `&self` fns on the inner value as
        // we never perform unsafe mutations.
        unsafe { &*self.inner.get() }
    }
}

impl ops::DerefMut for AtomicIsize {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // safety: we hold `&mut self`
        unsafe { &mut *self.inner.get() }
    }
}

impl fmt::Debug for AtomicIsize {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(fmt)
    }
}
