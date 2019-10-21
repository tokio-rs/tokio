use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use super::ScheduledIo;
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};

#[derive(Debug)]
pub(crate) struct Slot {
    gen: AtomicUsize,
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    /// The data stored in the slot.
    item: ScheduledIo,
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

impl Slot {
    pub(super) fn new(next: usize) -> Self {
        Self {
            gen: AtomicUsize::new(0),
            item: ScheduledIo::default(),
            next: CausalCell::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn get(&self, gen: Generation) -> Option<&ScheduledIo> {
        let current = self.gen.load(Ordering::Acquire);
        test_println!("-> get {:?}; current={:?}", gen, current);

        // Is the index's generation the same as the current generation? If not,
        // the item that index referred to was removed, so return `None`.
        if gen.value != current {
            return None;
        }

        Some(&self.item)
    }

    #[inline(always)]
    pub(in crate::driver::reactor::sharded_slab) fn value(&self) -> Option<&ScheduledIo> {
        Some(&self.item)
    }

    #[inline]
    pub(super) fn alloc(&self) -> Generation {
        Generation::from_usize(self.gen.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn reset(&self, gen: Generation) -> bool {
        let next = (gen.value + 1) % Generation::BITS;
        let actual = self.gen.compare_and_swap(gen.value, next, Ordering::AcqRel);
        test_println!("-> remove {:?}; next={:?}; actual={:?}", gen, next, actual);
        if actual != gen.value {
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
