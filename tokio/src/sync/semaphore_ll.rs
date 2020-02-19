use crate::loom::{
    alloc,
    future::AtomicWaker,
    sync::{atomic::AtomicUsize, Mutex},
};
use crate::util::linked_list::{self, LinkedList};

use std::{
    cmp,
    fmt,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::Ordering,
    task::{
        Context, Poll,
        Poll::{Pending, Ready},
    },
};

pub(crate) struct Semaphore {
    waiters: Mutex<LinkedList<Waiter>>,
    permits: AtomicUsize,
    add_lock: AtomicUsize,
}

/// Error returned by `Permit::try_acquire`.
#[derive(Debug)]
pub(crate) enum TryAcquireError {
    Closed,
    NoPermits,
}
/// Error returned by `Permit::poll_acquire`.
#[derive(Debug)]
pub(crate) struct AcquireError(());

pin_project_lite::pin_project! {
    #[derive(Debug)]
    pub(crate) struct Permit {
        #[pin]
        node: linked_list::Entry<Waiter>,
        state: PermitState,
    }
}

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
    state: AtomicUsize,
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
            add_lock: AtomicUsize::new(0),
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        // Acquire the `add_lock`, setting the "closed" flag on the lock.
        let prev = self.add_lock.fetch_or(1, Ordering::AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        self.add_permits_locked(0, true);
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, n: usize) {
        if n == 0 {
            return;
        }

        // TODO: Handle overflow. A panic is not sufficient, the process must
        // abort.
        let prev = self.add_lock.fetch_add(n << 1, Ordering::AcqRel);
        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }
        self.add_permits_locked(n, false);
    }

    fn add_permits_locked(&self, mut rem: usize, mut closed: bool) {
        while rem > 0 || closed {

            // Release the permits and notify
            let mut waiters = self.waiters.lock().unwrap();
            while rem > 0 {
                let pop = match waiters.last() {
                    Some(last) => last.assign_permits(&mut rem, closed),
                    None => {
                        self.permits.fetch_add(rem, Ordering::Release);
                        break;
                    }
                };
                if pop {
                    waiters.pop_back().unwrap();
                }
            }

            let n = rem << 1;

            let actual = if closed {
                let actual = self.add_lock.fetch_sub(n | 1, Ordering::AcqRel);
                closed = false;
                actual
            } else {
                let actual = self.add_lock.fetch_sub(n, Ordering::AcqRel);
                closed = actual & 1 == 1;
                actual
            };

            rem = (actual >> 1) - rem;
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        needed: u16,
        node: Pin<&mut Waiter>,
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
            let mut waiter = unsafe { &*node.get() };
            waiter.state.compare_exchange(0, remaining, Ordering::Release).expect("unlinked node should not want permits!");
            waiter.waker.register_by_ref(cx.waker());
        }

        let mut queue = self.waiters.lock().unwrap();
        unsafe {
            queue.push_front(node);
        }

        Pending
    }
}

impl Permit {
    /// Creates a new `Permit`.
    ///
    /// The permit begins in the "unacquired" state.
    pub(crate) fn new() -> Permit {
        use PermitState::Acquired;

        Permit {
            node: linked_list::Entry::new(Waiter {
                waker: AtomicWaker::new(),
                needed: 0,
            }),
            state: Acquired(0),
        }
    }

    /// Returns `true` if the permit has been acquired
    pub(crate) fn is_acquired(&self) -> bool {
        match self.state {
            PermitState::Acquired(num) if num > 0 => true,
            _ => false,
        }
    }


    /// Tries to acquire the permit. If no permits are available, the current task
    /// is notified once a new permit becomes available.
    pub(crate) fn poll_acquire(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Poll<Result<(), AcquireError>> {
        semaphore.poll_acquire(cx, num_permits, self.project().node)
    }

    /// Tries to acquire the permit.
    pub(crate) fn try_acquire(
        &mut self,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Result<(), TryAcquireError> {
        unimplemented!()
    }

    /// Releases a permit back to the semaphore
    pub(crate) fn release(&mut self, n: u16, semaphore: &Semaphore) {
        let n = self.forget(n, semaphore);
        semaphore.add_permits(n as usize);
    }

    /// Forgets the permit **without** releasing it back to the semaphore.
    ///
    /// After calling `forget`, `poll_acquire` is able to acquire new permit
    /// from the sempahore.
    ///
    /// Repeatedly calling `forget` without associated calls to `add_permit`
    /// will result in the semaphore losing all permits.
    ///
    /// Will forget **at most** the number of acquired permits. This number is
    /// returned.
    pub(crate) fn forget(&mut self, n: u16, semaphore: &Semaphore) -> u16 {
        use PermitState::*;

        match self.state {
            Waiting(requested) => {
                let n = cmp::min(n, requested);

                // Decrement
                let acquired = self
                    .node
                    .try_dec_permits_to_acquire(n as usize) as u16;

                if n == requested {
                    self.state = Acquired(0);
                    // TODO: rm from wait list here!
                    semaphore
                } else if acquired == requested - n {
                    self.state = Waiting(acquired);
                } else {
                    self.state = Waiting(requested - n);
                }

                acquired
            }
            Acquired(acquired) => {
                let n = cmp::min(n, acquired);
                self.state = Acquired(acquired - n);
                n
            }
        }
    }
}

impl Default for Permit {
    fn default() -> Self {
        Self::new()
    }
}

impl Waiter {
    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize, closed: bool) -> bool {
        let mut curr = self.state.load(Ordering::Acquire);

        loop {
            // Number of permits to assign to this waiter
            let assign = cmp::min(curr, *n);
            let next = curr - assign;
            match self.state.compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    // Update `n`
                    *n -= assign;

                    if next == 0 {
                        if curr > 0 {
                            self.waker.wake();
                        }

                        return true;
                    } else {
                        return false;
                    }
                }
                Err(actual) => curr = actual,
            }
        }
    }

    /// Try to decrement the number of permits to acquire. This returns the
    /// actual number of permits that were decremented. The delta betweeen `n`
    /// and the return has been assigned to the permit and the caller must
    /// assign these back to the semaphore.
    fn try_dec_permits_to_acquire(&self, n: usize) -> usize {
        let mut curr = self.state.load(Ordering::Acquire);

        loop {
            if !curr.is_queued() {
                assert_eq!(0, curr.permits_to_acquire());
            }

            let delta = cmp::min(n, curr.permits_to_acquire());
            let rem = curr.permits_to_acquire() - delta;

            match self.state.compare_exchange(curr, rem, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return n - delta,
                Err(actual) => curr = actual,
            }
        }
    }
}

// ===== impl AcquireError ====

impl AcquireError {
    fn closed() -> AcquireError {
        AcquireError(())
    }
}

fn to_try_acquire(_: AcquireError) -> TryAcquireError {
    TryAcquireError::Closed
}

impl fmt::Display for AcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "semaphore closed")
    }
}

impl std::error::Error for AcquireError {}

// ===== impl TryAcquireError =====

impl TryAcquireError {
    /// Returns `true` if the error was caused by a closed semaphore.
    pub(crate) fn is_closed(&self) -> bool {
        match self {
            TryAcquireError::Closed => true,
            _ => false,
        }
    }

    /// Returns `true` if the error was caused by calling `try_acquire` on a
    /// semaphore with no available permits.
    pub(crate) fn is_no_permits(&self) -> bool {
        match self {
            TryAcquireError::NoPermits => true,
            _ => false,
        }
    }
}

impl fmt::Display for TryAcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TryAcquireError::Closed => write!(fmt, "{}", "semaphore closed"),
            TryAcquireError::NoPermits => write!(fmt, "{}", "no permits available"),
        }
    }
}

impl std::error::Error for TryAcquireError {}
