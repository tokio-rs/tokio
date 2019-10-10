use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use std::fmt;

pub(crate) struct Slot<T> {
    /// ABA guard generation counter incremented every time a value is inserted
    /// into the slot.
    gen: AtomicUsize,
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct Generation {
    value: usize,
}

impl Pack for Generation {
    /// Use all the remaining bits in the word for the generation counter, minus
    /// any bits reserved by the user.
    const LEN: usize = (WIDTH - RESERVED_BITS) - Self::SHIFT;

    type Prev = Tid;

    #[inline(always)]
    fn from_usize(u: usize) -> Self {
        debug_assert!(u <= Self::BITS);
        Self::new(u)
    }

    #[inline(always)]
    fn as_usize(&self) -> usize {
        self.value
    }
}

impl Generation {
    fn new(value: usize) -> Self {
        Self { value }
    }
}

impl<T> Slot<T> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            gen: AtomicUsize::new(0),
            item: CausalCell::new(None),
            next: CausalCell::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn get(&self, gen: Generation) -> Option<&T> {
        let current = self.gen.load(Ordering::Acquire);
        #[cfg(test)]
        println!("-> get {:?}; current={:?}", gen, current);

        // Is the index's generation the same as the current generation? If not,
        // the item that index referred to was removed, so return `None`.
        if gen.value != current {
            return None;
        }

        self.value()
    }

    #[inline(always)]
    pub(super) fn value<'a>(&'a self) -> Option<&'a T> {
        self.item.with(|item| unsafe { (&*item).as_ref() })
    }

    #[inline]
    pub(super) fn insert(&self, value: &mut Option<T>) -> Generation {
        debug_assert!(
            self.item.with(|item| unsafe { (*item).is_none() }),
            "inserted into full slot"
        );
        debug_assert!(value.is_some(), "inserted twice");

        // Set the new value.
        self.item.with_mut(|item| unsafe {
            *item = value.take();
        });

        let gen = self.gen.load(Ordering::Acquire);

        #[cfg(test)]
        println!("-> {:?}", gen);

        Generation::new(gen)
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn remove_value(&self, gen: Generation) -> Option<T> {
        let current = self.gen.load(Ordering::Acquire);

        #[cfg(test)]
        println!("-> remove={:?}; current={:?}", gen, current);

        // Is the index's generation the same as the current generation? If not,
        // the item that index referred to was already removed.
        if gen.value != current {
            return None;
        }

        let next_gen = (current + 1) % Generation::BITS;

        #[cfg(test)]
        println!("-> new gen {:?}", next_gen);

        if self
            .gen
            .compare_and_swap(current, next_gen, Ordering::Release)
            != current
        {
            #[cfg(test)]
            println!("-> already removed!");
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
        f.debug_struct("Slot")
            .field("gen", &self.gen.load(Ordering::Relaxed))
            .field("next", &self.next())
            .finish()
    }
}
