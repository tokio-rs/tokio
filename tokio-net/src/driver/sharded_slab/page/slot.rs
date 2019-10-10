use crate::sync::CausalCell;
use std::fmt;

pub(crate) struct Slot<T> {
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
}

impl<T> Slot<T> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            item: CausalCell::new(None),
            next: CausalCell::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn value<'a>(&'a self) -> Option<&'a T> {
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
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn remove_value(&self) -> Option<T> {
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
