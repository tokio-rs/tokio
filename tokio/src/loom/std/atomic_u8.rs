use std::cell::UnsafeCell;
use std::fmt;
use std::ops::Deref;
use std::panic;

/// `AtomicU8` providing an additional `with_mut` function.
pub(crate) struct AtomicU8 {
    inner: std::sync::atomic::AtomicU8,
}

impl AtomicU8 {
    pub(crate) const fn new(val: u8) -> AtomicU8 {
        let inner = std::sync::atomic::AtomicU8::new(val);
        AtomicU8 { inner }
    }

    /// Get access to a mutable reference to the inner value.
    pub(crate) fn with_mut<R>(&mut self, f: impl FnOnce(&mut u8) -> R) -> R {
        f(self.inner.get_mut())
    }
}

impl Deref for AtomicU8 {
    type Target = std::sync::atomic::AtomicU8;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl fmt::Debug for AtomicU8 {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(fmt)
    }
}
