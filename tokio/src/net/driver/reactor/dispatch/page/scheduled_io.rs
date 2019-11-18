use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use crate::loom::{cell::CausalCell, sync::atomic::AtomicUsize};
use crate::sync::AtomicWaker;

use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct ScheduledIo {
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    readiness: AtomicUsize,
    pub(in crate::net::driver) reader: AtomicWaker,
    pub(in crate::net::driver) writer: AtomicWaker,
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
    const ONE: usize = 1 << Self::SHIFT;

    fn new(value: usize) -> Self {
        Self { value }
    }

    fn next(self) -> Self {
        Self::from_usize((self.value + 1) % Self::BITS)
    }
}

impl ScheduledIo {
    pub(super) fn new(next: usize) -> Self {
        Self {
            next: CausalCell::new(next),
            readiness: AtomicUsize::new(0),
            reader: AtomicWaker::new(),
            writer: AtomicWaker::new(),
        }
    }

    #[inline]
    pub(super) fn alloc(&self) -> Generation {
        Generation::from_packed(self.readiness.load(Ordering::SeqCst))
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn reset(&self, gen: Generation) -> bool {
        let mut current = self.readiness.load(Ordering::Acquire);
        loop {
            if Generation::from_packed(current) != gen {
                return false;
            }
            let next_gen = gen.next().pack(0);
            match self.readiness.compare_exchange(
                current,
                next_gen,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        drop(self.reader.take_waker());
        drop(self.writer.take_waker());
        true
    }

    #[inline(always)]
    pub(super) fn set_next(&self, next: usize) {
        self.next.with_mut(|n| unsafe {
            (*n) = next;
        })
    }

    /// Returns the current readiness value of this `ScheduledIo`, if the
    /// provided `token` is still a valid access.
    ///
    /// # Returns
    ///
    /// If the given token's generation no longer matches the `ScheduledIo`'s
    /// generation, then the corresponding IO resource has been removed and
    /// replaced with a new resource. In that case, this method returns `None`.
    /// Otherwise, this returns the current readiness.
    pub(in crate::net::driver) fn get_readiness(&self, token: usize) -> Option<usize> {
        let gen = token & Generation::MASK;
        let ready = self.readiness.load(Ordering::Acquire);
        if ready & Generation::MASK != gen {
            return None;
        }
        Some(ready & (!Generation::MASK))
    }

    /// Sets the readiness on this `ScheduledIo` by invoking the given closure on
    /// the current value, returning the previous readiness value.
    ///
    /// # Arguments
    /// - `token`: the token for this `ScheduledIo`.
    /// - `f`: a closure returning a new readiness value given the previous
    ///   readiness.
    ///
    /// # Returns
    ///
    /// If the given token's generation no longer matches the `ScheduledIo`'s
    /// generation, then the corresponding IO resource has been removed and
    /// replaced with a new resource. In that case, this method returns `Err`.
    /// Otherwise, this returns the previous readiness.
    pub(in crate::net::driver) fn set_readiness(
        &self,
        token: usize,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let gen = token & Generation::MASK;
        let mut current = self.readiness.load(Ordering::Acquire);
        loop {
            // Check that the generation for this access is still the current
            // one.
            if current & Generation::MASK != gen {
                return Err(());
            }
            // Mask out the generation bits so that the modifying function
            // doesn't see them.
            let current_readiness = current & mio::Ready::all().as_usize();
            let new = f(current_readiness);
            debug_assert!(
                new < Generation::ONE,
                "new readiness value would overwrite generation bits!"
            );

            match self.readiness.compare_exchange(
                current,
                new | gen,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(current),
                // we lost the race, retry!
                Err(actual) => current = actual,
            }
        }
    }
}
