use super::super::{Pack, Tid, RESERVED_BITS, WIDTH};
use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};

use tokio_sync::AtomicWaker;

#[derive(Debug)]
pub(crate) struct ScheduledIo {
    /// The offset of the next item on the free list.
    next: CausalCell<usize>,
    readiness: AtomicUsize,
    pub(in crate::driver) reader: AtomicWaker,
    pub(in crate::driver) writer: AtomicWaker,
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
            let current_gen = Generation::from_packed(current);
            test_println!("-> reset gen={:?}; current={:?};", gen, current);
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
            match self.readiness.compare_exchange(
                current,
                next,
                Ordering::Release,
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

    pub(in crate::driver) fn get_readiness(&self, token: usize) -> Option<usize> {
        let gen = token & Generation::MASK;
        let ready = self.readiness.load(Ordering::Acquire);
        test_println!("--> get_readiness: gen={:#x}; ready={:#x};", gen, ready);
        let actual_gen = ready & Generation::MASK;
        if actual_gen != gen {
            test_println!(
                "--> get_readiness: wrong generation {:#x}; expected={:x};",
                actual_gen,
                gen
            );
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
        let mut current = self.readiness.load(Ordering::Acquire);
        test_println!(
            "set_readiness: token={:#x}; gen={:#x}; current={:#x};",
            token,
            gen,
            current
        );

        loop {
            let current_gen = current & Generation::MASK;
            if current_gen != gen {
                test_println!("--> wrong generation {:#x}", current_gen);
                return Err(());
            }
            let new = f(current & mio::Ready::all().as_usize());
            debug_assert!(new < Generation::ONE);
            let new = new | gen;
            match self
                .readiness
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_actual) => {
                    debug_assert_eq!(_actual, current);
                    test_println!("--> set readiness {:#x};", new);
                    return Ok(new);
                }
                Err(actual) => {
                    test_println!(
                        "--> set_readiness failed; actual={:#x}; actual_gen={:#x}; gen={:#x}",
                        actual,
                        actual & Generation::MASK;,
                        gen
                    );
                    current = actual
                }
            }
        }
    }
}
