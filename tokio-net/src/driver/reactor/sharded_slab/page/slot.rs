use super::ScheduledIo;
use crate::sync::{
    atomic::{AtomicBool, Ordering},
    CausalCell,
};

#[derive(Debug)]
pub(crate) struct Slot {
    empty: AtomicBool,
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    /// The data stored in the slot.
    item: ScheduledIo,
}

impl Slot {
    pub(super) fn new(next: usize) -> Self {
        Self {
            empty: AtomicBool::new(true),
            item: ScheduledIo::default(),
            next: CausalCell::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn value(&self) -> Option<&ScheduledIo> {
        if self.empty.load(Ordering::Acquire) {
            return None;
        }
        Some(&self.item)
    }

    #[inline]
    pub(super) fn insert(&self, aba_guard: usize) {
        // Set the new value.
        self.item.insert(aba_guard);

        let was_empty = self.empty.compare_and_swap(true, false, Ordering::Release);
        assert!(was_empty, "slot must have been empty!")
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn reset(&self) -> bool {
        let is_empty = self.empty.compare_and_swap(false, true, Ordering::Release);
        if is_empty {
            return false;
        };

        self.item.reset();
        true
    }

    #[inline(always)]
    pub(super) fn set_next(&self, next: usize) {
        self.next.with_mut(|n| unsafe {
            (*n) = next;
        })
    }
}
