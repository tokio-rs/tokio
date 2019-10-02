use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use std::fmt;

pub(crate) struct Slot<T> {
    /// ABA guard generation counter incremented every time a value is inserted
    /// into the slot.
    gen: Generation,
    /// The offset of the next item on the free list.
    next: AtomicUsize,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Generation(usize);

impl Pack for Generation {
    /// Use all the remaining bits in the word for the generation counter, minus
    /// the reserved bits.
    const LEN: usize = (WIDTH - RESERVED_BITS) - Self::SHIFT;

    type Prev = Tid;

    #[inline(always)]
    fn from_usize(u: usize) -> Self {
        debug_assert!(u <= Self::BITS);
        Self(u)
    }

    #[inline(always)]
    fn as_usize(&self) -> usize {
        self.0
    }
}

impl Generation {
    #[inline(always)]
    fn advance(&mut self) -> Self {
        self.0 = (self.0 + 1) % Self::BITS;
        debug_assert!(self.0 <= Self::BITS);
        *self
    }
}

impl<T> Slot<T> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            gen: Generation(0),
            item: CausalCell::new(None),
            next: AtomicUsize::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn get(&self, gen: Generation) -> Option<&T> {
        // Is the index's generation the same as the current generation? If not,
        // the item that index referred to was removed, so return `None`.
        if gen != self.gen {
            return None;
        }

        self.value()
    }

    #[inline(always)]
    pub(super) fn value<'a>(&'a self) -> Option<&'a T> {
        self.item.with(|item| unsafe { (&*item).as_ref() })
    }

    #[inline(always)]
    pub(super) fn insert(&mut self, value: &mut Option<T>) -> Generation {
        debug_assert!(
            self.item.with(|item| unsafe { (*item).is_none() }),
            "inserted into full slot"
        );
        debug_assert!(value.is_some(), "inserted twice");

        // Set the new value.
        self.item.with_mut(|item| unsafe {
            *item = value.take();
        });

        // Advance the slot's generation by one, returning the new generation.
        self.gen.advance()
    }

    pub(super) fn next(&self) -> usize {
        self.next.load(Ordering::Acquire)
    }

    pub(super) fn remove(&self, gen: Generation, next: usize) -> Option<T> {
        // Is the index's generation the same as the current generation? If not,
        // the item that index referred to was already removed.
        if gen != self.gen {
            return None;
        }

        let val = self.item.with_mut(|item| unsafe { (*item).take() });
        debug_assert!(val.is_some());

        self.next.store(next, Ordering::Release);
        val
    }
}

impl<T> fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slot")
            .field("gen", &self.gen)
            .field(
                "next",
                &format_args!("{:#0x}", self.next.load(Ordering::Relaxed)),
            )
            .finish()
    }
}
