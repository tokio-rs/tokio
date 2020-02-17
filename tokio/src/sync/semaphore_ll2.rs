use crate::loom::{
    alloc,
    future::AtomicWaker,
    sync::{atomic::AtomicUsize, Mutex},
};
use crate::util::linked_list::{self, LinkedList};

use std::{
    pin::Pin,
    ptr::NonNull,
    sync::atomic::Ordering,
    task::{
        Context, Poll,
        Poll::{Pending, Ready},
    },
};

pub(crate) struct Semaphore {
    waiters: Mutex<LinkedList<alloc::Track<Waiter>>>,
    permits: AtomicUsize,
}

/// Error returned by `Permit::poll_acquire`.
#[derive(Debug)]
pub(crate) struct AcquireError(());

pin_project_lite::pin_project! {
    pub(crate) struct Permit {
        #[pin]
        node: WaitNode,
        state: PermitState,
    }
}

type WaitNode = linked_list::Entry<alloc::Track<Waiter>>;

/// Permit state
#[derive(Debug, Copy, Clone)]
enum PermitState {
    /// Currently waiting for permits to be made available and assigned to the
    /// waiter.
    Waiting(u16),

    /// The number of acquired permits
    Acquired(u16),
}

struct Waiter {
    waker: AtomicWaker,
    needed: u16,
}

const CLOSED: usize = std::usize::MAX;

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    ///
    /// # Panics
    ///
    /// Panics if `permits` is zero.
    pub(crate) fn new(permits: usize) -> Self {
        assert!(permits > 0, "number of permits must be greater than 0");
        Self {
            permits: AtomicUsize::new(permits),
            waiters: Mutex::new(LinkedList::new()),
        }
    }
    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        needed: u16,
        node: Pin<&mut WaitNode>,
    ) -> Poll<Result<(), AcquireError>> {
        let mut curr = self.permits.load(Ordering::Acquire);
        let remaining = loop {
            if curr == CLOSED {
                return Ready(Err(AcquireError(())));
            }
            let next = curr.saturating_sub(needed as usize);
            match self.permits.compare_exchange_weak(
                curr,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) if curr >= needed as usize => return Ready(Ok(())),
                Ok(_) => break needed - curr as u16,
                Err(actual) => curr = actual,
            }
        };
        // if we've not returned already, then we need to push the waiter to the
        // wait queue.
        assert!(node.is_unlinked());
        {
            let mut waiter = unsafe { &mut *node.get() }.get_mut();
            waiter.needed = remaining;
            waiter.waker.register_by_ref(cx.waker());
        }

        let mut queue = self.waiters.lock().unwrap();
        unsafe {
            queue.push_front(node);
        }

        Pending
    }
}
