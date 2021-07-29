use crate::loom::cell::UnsafeCell;

use std::collections::VecDeque;
use std::marker::PhantomData;

/// This type is like VecDeque, except that it is not Sync and can be modified
/// through immutable references.
pub(crate) struct VecDequeCell<T> {
    inner: UnsafeCell<VecDeque<T>>,
    _not_sync: PhantomData<*const ()>,
}

// This is Send for the same reasons that RefCell<VecDeque<T>> is Send.
unsafe impl<T: Send> Send for VecDequeCell<T> {}

impl<T> VecDequeCell<T> {
    pub(crate) fn with_capacity(cap: usize) -> Self {
        Self {
            inner: UnsafeCell::new(VecDeque::with_capacity(cap)),
            _not_sync: PhantomData,
        }
    }

    #[inline]
    fn with_inner<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut VecDeque<T>) -> R,
    {
        // safety: This type is not Sync, so concurrent calls of this method
        // can't happen.  Furthermore, all uses of this method in this file make
        // sure that they don't call `with_inner` recursively.
        self.inner.with_mut(|ptr| unsafe { f(&mut *ptr) })
    }

    pub(crate) fn pop_front(&self) -> Option<T> {
        self.with_inner(VecDeque::pop_front)
    }

    pub(crate) fn push_back(&self, item: T) {
        self.with_inner(|inner| inner.push_back(item));
    }

    /// Replace the inner VecDeque with an empty VecDeque and return the current
    /// contents.
    pub(crate) fn take(&self) -> VecDeque<T> {
        self.with_inner(|inner| std::mem::take(inner))
    }
}
