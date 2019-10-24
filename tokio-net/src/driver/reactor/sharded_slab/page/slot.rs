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
    pub(crate) const ONE: usize = 1 << Self::SHIFT;
    fn new(value: usize) -> Self {
        Self { value }
    }

    pub(crate) fn next(self) -> Self {
        Self::from_usize((self.value + 1) % Self::BITS)
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
    pub(in crate::driver::reactor::sharded_slab) fn value(&self) -> Option<&ScheduledIo> {
        Some(&self.item)
    }

    #[inline]
    pub(super) fn alloc(&self) -> Generation {
        Generation::from_packed(self.item.readiness.load(Ordering::SeqCst))
    }

    #[inline(always)]
    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    #[inline]
    pub(super) fn reset(&self, gen: Generation) -> bool {
        test_println!(
            "dump gen\n\tGEN_MASK={:#b};\n\tGEN_BITS={:#b};",
            Generation::MASK,
            Generation::BITS
        );
        let mut current = self.item.readiness.load(Ordering::Acquire);
        loop {
            let current_gen = Generation::from_packed(current);
            test_println!("-> reset gen={:?}; current={:?};", gen, current,);
            if current_gen != gen {
                return false;
            }
            let next_gen = gen.next();
            let next = next_gen.pack(0);
            test_println!(
                "-> reset current={:#x}; next={:?}; packed={:#x};",
                current,
                next_gen,
                next
            );
            match self.item.readiness.compare_exchange(
                current,
                next,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    test_println!("-> reset!");
                    break;
                }
                Err(actual) => {
                    test_println!("-> retry");
                    current = actual;
                }
            }
        }
        drop(self.item.reader.take_waker());
        drop(self.item.writer.take_waker());
        true
    }

    #[inline(always)]
    pub(super) fn set_next(&self, next: usize) {
        self.next.with_mut(|n| unsafe {
            (*n) = next;
        })
    }

    pub(in crate::driver) fn get_readiness(&self, token: usize) -> Option<usize> {
        let gen = token & Generation::MASK;
        let ready = self.item.readiness.load(Ordering::SeqCst);
        test_println!(
            "get_readiness:\n\tMASK= {:#064b}\n\tGEN = {:#064b}\n\tRDY = {:#064b}",
            Generation::MASK,
            gen,
            ready
        );
        if ready & Generation::MASK != gen {
            return None;
        }
        Some(ready & (!Generation::MASK))
    }

    pub(in crate::driver) fn set_readiness(
        &self,
        token: usize,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let gen = token & Generation::MASK;
        let mut current = self.item.readiness.load(Ordering::Acquire);
        loop {
            let new = f(current & mio::Ready::all().as_usize());
            debug_assert!(new < Generation::ONE);
            match self.item.readiness.compare_exchange(
                current,
                new | gen,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(new),
                Err(actual) if actual & Generation::MASK != gen => return Err(()),
                Err(actual) => current = actual,
            }
        }
    }

    pub(in crate::driver) fn io(&self) -> &ScheduledIo {
        &self.item
    }
}
