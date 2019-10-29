use super::{page, Pack};
use std::{
    cell::{Cell, UnsafeCell},
    collections::VecDeque,
    fmt,
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use lazy_static::lazy_static;

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

/// Tracks any thread IDs that can be reused, and a monotonic counter for
/// generating new thread IDs.
struct Registry {
    /// The next thread ID number; used when there are no free IDs.
    next: AtomicUsize,
    /// A queue of thread IDs whose threads have terminated. These will be
    /// reused if possible.
    free: Mutex<VecDeque<usize>>,
}

lazy_static! {
    static ref REGISTRY: Registry = Registry {
        next: AtomicUsize::new(0),
        free: Mutex::new(VecDeque::new()),
    };
}
loom_thread_local! {
    static REGISTRATION: Registration = Registration::new();
}

// === impl Tid ===

impl Pack for Tid {
    const LEN: usize = super::MAX_THREADS.trailing_zeros() as usize + 1;

    type Prev = page::Addr;

    #[inline(always)]
    fn as_usize(&self) -> usize {
        self.id
    }

    #[inline(always)]
    fn from_usize(id: usize) -> Self {
        debug_assert!(id <= Self::BITS);
        Self {
            id,
            _not_send: PhantomData,
        }
    }
}

impl Tid {
    #[inline]
    pub(crate) fn current() -> Self {
        REGISTRATION
            .try_with(Registration::current)
            .unwrap_or_else(|_| Self::poisoned())
    }

    pub(crate) fn is_current(self) -> bool {
        REGISTRATION
            .try_with(|r| self == r.current())
            .unwrap_or(false)
    }

    #[inline(always)]
    pub(crate) fn new(id: usize) -> Self {
        Self {
            id,
            _not_send: PhantomData,
        }
    }

    #[cold]
    fn poisoned() -> Self {
        Self {
            id: std::usize::MAX,
            _not_send: PhantomData,
        }
    }

    /// Returns true if the local thread ID was accessed while unwinding.
    pub(crate) fn is_poisoned(self) -> bool {
        self.id == std::usize::MAX
    }
}

impl fmt::Debug for Tid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_poisoned() {
            f.debug_tuple("Tid")
                .field(&format_args!("<poisoned>"))
                .finish()
        } else {
            f.debug_tuple("Tid")
                .field(&format_args!("{:#x}", self.id))
                .finish()
        }
    }
}

// === impl Registration ===

impl Registration {
    fn new() -> Self {
        Self(Cell::new(None))
    }

    #[inline(always)]
    fn current(&self) -> Tid {
        if let Some(tid) = self.0.get().map(Tid::new) {
            tid
        } else {
            self.register()
        }
    }

    #[cold]
    fn register(&self) -> Tid {
        let id = REGISTRY
            .free
            .lock()
            .ok()
            .and_then(|mut free| {
                if free.len() > 1 {
                    free.pop_front()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| REGISTRY.next.fetch_add(1, Ordering::AcqRel));
        debug_assert!(id <= Tid::BITS, "thread ID overflow!");
        self.0.set(Some(id));
        Tid::new(id)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(id) = self.0.get() {
            if let Ok(mut free) = REGISTRY.free.lock() {
                free.push_back(id);
            }
        }
    }
}
