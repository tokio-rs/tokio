use super::super::{cfg, Pack, Tid};
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use std::{fmt, marker::PhantomData};

pub(crate) struct Slot<T, C> {
    /// ABA guard generation counter incremented every time a value is inserted
    /// into the slot.
    gen: AtomicUsize,
    /// The offset of the next item on the free list.
    next: AtomicUsize,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
    _cfg: PhantomData<fn(C)>,
}

#[repr(transparent)]
pub(crate) struct Generation<C = cfg::DefaultConfig> {
    value: usize,
    _cfg: PhantomData<fn(C)>,
}

impl<C: cfg::Config> Pack<C> for Generation<C> {
    /// Use all the remaining bits in the word for the generation counter, minus
    /// any bits reserved by the user.
    const LEN: usize = (cfg::WIDTH - C::RESERVED_BITS) - Self::SHIFT;
    const BITS: usize = cfg::make_mask(Self::LEN);

    type Prev = Tid<C>;

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

impl<C: cfg::Config> Generation<C> {
    fn new(value: usize) -> Self {
        Self {
            value,
            _cfg: PhantomData,
        }
    }
}

impl<T, C: cfg::Config> Slot<T, C> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            gen: AtomicUsize::new(0),
            item: CausalCell::new(None),
            next: AtomicUsize::new(next),
            _cfg: PhantomData,
        }
    }

    #[inline(always)]
    pub(super) fn get(&self, gen: Generation<C>) -> Option<&T> {
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
    pub(super) fn insert(&self, value: &mut Option<T>) -> Generation<C> {
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
        self.next.load(Ordering::Acquire)
    }

    #[inline]
    pub(super) fn remove_value(&self, gen: Generation<C>) -> Option<T> {
        let current = self.gen.load(Ordering::Acquire);

        #[cfg(test)]
        println!("-> remove={:?}; current={:?}", gen, current);
        let next_gen = (current + 1) % Generation::<C>::BITS;

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
        self.next.store(next, Ordering::Release);
    }
}

impl<C, T> fmt::Debug for Slot<C, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slot")
            .field("gen", &self.gen.load(Ordering::Relaxed))
            .field("next", &self.next.load(Ordering::Relaxed))
            .finish()
    }
}

impl<C> fmt::Debug for Generation<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Generation").field(&self.value).finish()
    }
}

impl<C: cfg::Config> PartialEq for Generation<C> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<C: cfg::Config> Eq for Generation<C> {}

impl<C: cfg::Config> PartialOrd for Generation<C> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<C: cfg::Config> Ord for Generation<C> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<C: cfg::Config> Clone for Generation<C> {
    fn clone(&self) -> Self {
        Self::new(self.value)
    }
}

impl<C: cfg::Config> Copy for Generation<C> {}
