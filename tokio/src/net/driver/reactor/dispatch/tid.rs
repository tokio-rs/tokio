use super::{page, Pack};
use std::{
    cell::{Cell, UnsafeCell},
    fmt,
    marker::PhantomData,
};

/// Uniquely identifies a thread.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) struct Tid {
    id: usize,
    _not_send: PhantomData<UnsafeCell<()>>,
}

/// Registers that a thread is currently using a thread ID.
///
/// This is stored in a thread local on each thread that has been assigned an
/// ID. When the thread terminates, the thread local is dropped, indicating that
/// that thread's ID number may be reused. This is to avoid exhausting the
/// available bits for thread IDs in scenarios where threads are spawned and
/// terminated very frequently.
#[derive(Debug)]
struct Registration(Cell<Option<usize>>);

// === impl Tid ===

impl Pack for Tid {
    const LEN: usize = super::MAX_THREADS.trailing_zeros() as usize + 1;

    type Prev = page::Addr;

    fn as_usize(&self) -> usize {
        self.id
    }

    fn from_usize(id: usize) -> Self {
        debug_assert!(id <= Self::BITS);

        Tid {
            id,
            _not_send: PhantomData,
        }
    }
}

impl fmt::Debug for Tid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Tid")
            .field(&format_args!("{:#x}", self.id))
            .finish()
    }
}
