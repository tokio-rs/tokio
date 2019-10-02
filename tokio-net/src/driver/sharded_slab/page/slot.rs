use super::{
    cfg::{self, CfgPrivate},
    Pack, Tid,
};
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use std::{fmt, marker::PhantomData};

pub(crate) struct Slot<T, C> {
    /// ABA guard generation counter incremented every time a value is inserted
    /// into the slot.
    gen: Generation<C>,
    /// The offset of the next item on the free list.
    next: AtomicUsize,
    /// The data stored in the slot.
    item: CausalCell<Option<T>>,
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

    #[inline(always)]
    fn advance(&mut self) -> Self {
        #[cfg(test)]
        print!("-> advance gen {:?}", self.value);
        self.value = (self.value + 1) % Self::BITS;
        #[cfg(test)]
        println!(" to {:?}", self.value);
        debug_assert!(self.value <= Self::BITS);
        *self
    }
}

impl<T, C: cfg::Config> Slot<T, C> {
    pub(super) fn new(next: usize) -> Self {
        Self {
            gen: Generation::new(0),
            item: CausalCell::new(None),
            next: AtomicUsize::new(next),
        }
    }

    #[inline(always)]
    pub(super) fn get(&self, gen: Generation<C>) -> Option<&T> {
        #[cfg(test)]
        println!("-> get {:?}; current={:?}", gen, self.gen);

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
    pub(super) fn insert(&mut self, value: &mut Option<T>) -> Generation<C> {
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
        let gen = self.gen.advance();
        #[cfg(test)]
        println!("-> {:?}", gen);
        gen
    }

    pub(super) fn next(&self) -> usize {
        self.next.load(Ordering::Acquire)
    }

    pub(super) fn remove(&self, gen: Generation<C>, next: usize) -> Option<T> {
        #[cfg(test)]
        println!("-> remove={:?}; current={:?}", gen, self.gen);

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

impl<C, T> fmt::Debug for Slot<C, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slot")
            .field("gen", &self.gen)
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
