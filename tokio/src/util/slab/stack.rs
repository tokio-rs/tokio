use crate::loom::sync::atomic::AtomicUsize;
use crate::util::slab::Address;

use std::fmt;
use std::sync::atomic::Ordering;
use std::usize;

pub(super) struct TransferStack {
    head: AtomicUsize,
}

impl TransferStack {
    pub(super) fn new() -> Self {
        Self {
            head: AtomicUsize::new(Address::NULL),
        }
    }

    pub(super) fn pop_all(&self) -> Option<usize> {
        let val = self.head.swap(Address::NULL, Ordering::Acquire);

        if val == Address::NULL {
            None
        } else {
            Some(val)
        }
    }

    pub(super) fn push(&self, value: usize, before: impl Fn(usize)) {
        let mut next = self.head.load(Ordering::Relaxed);

        loop {
            before(next);

            match self
                .head
                .compare_exchange(next, value, Ordering::AcqRel, Ordering::Acquire)
            {
                // lost the race!
                Err(actual) => next = actual,
                Ok(_) => return,
            }
        }
    }
}

impl fmt::Debug for TransferStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Loom likes to dump all its internal state in `fmt::Debug` impls, so
        // we override this to just print the current value in tests.
        f.debug_struct("TransferStack")
            .field(
                "head",
                &format_args!("{:#x}", self.head.load(Ordering::Relaxed)),
            )
            .finish()
    }
}
