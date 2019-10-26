use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use crate::loom::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};

use tokio_sync::AtomicWaker;

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

    pub(in crate::net::driver) fn get_readiness(&self, token: usize) -> Option<usize> {
        let gen = token & Generation::MASK;
        let ready = self.readiness.load(Ordering::Acquire);
        if ready & Generation::MASK != gen {
            return None;
        }
        Some(ready & (!Generation::MASK))
    }

    pub(in crate::net::driver) fn set_readiness(
        &self,
        token: usize,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let gen = token & Generation::MASK;
        let mut current = self.readiness.load(Ordering::Acquire);
        loop {
            if current & Generation::MASK != gen {
                return Err(());
            }
            let new = f(current & mio::Ready::all().as_usize());
            debug_assert!(new < Generation::ONE);
            match self.readiness.compare_exchange(
                current,
                new | gen,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(actual) => {
                    debug_assert_eq!(actual, current);
                    return Ok(actual);
                }
                Err(actual) => current = actual,
            }
        }
    }
}
