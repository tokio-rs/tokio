use crate::sync::{
    atomic::{AtomicBool, Ordering},
    CausalCell,
};
use std::fmt;

pub(crate) struct Slot<T> {
    empty: AtomicBool,
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
}

impl<T> Slot<T> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            empty: AtomicBool::new(true),
            item: CausalCell::new(None),
            next: CausalCell::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn value(&self) -> Option<&T> {
        if self.empty.load(Ordering::Acquire) {
            return None;
        }
        self.item.with(|item| unsafe { (&*item).as_ref() })
    }

    #[inline]
    pub(super) fn insert(&self, value: T) {
        debug_assert!(
            self.item.with(|item| unsafe { (*item).is_none() }),
            "inserted into full slot"
        );

        // Set the new value.
        self.item.with_mut(|item| unsafe {
            *item = Some(value);
        });

        let was_empty = self.empty.compare_and_swap(true, false, Ordering::Release);
        assert!(was_empty, "slot must have been empty!")
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn remove_value(&self) -> Option<T> {
        let is_empty = self.empty.compare_and_swap(false, true, Ordering::Release);
        if is_empty != false {
            return None;
        }
        let val = self.item.with_mut(|item| unsafe { (*item).take() });
        debug_assert!(val.is_some());
        val
    }

    #[inline(always)]
    pub(super) fn set_next(&self, next: usize) {
        self.next.with_mut(|n| unsafe {
            (*n) = next;
        })
    }
}

impl<T> fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slot").field("next", &self.next()).finish()
    }
}
