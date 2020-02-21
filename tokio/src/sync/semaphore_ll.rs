use crate::loom::{
    alloc,
    future::AtomicWaker,
    sync::{atomic::AtomicUsize, Mutex, MutexGuard},
};
use crate::util::linked_list::{self, LinkedList};

use std::{
    cmp, fmt,
    future::Future,
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

#[derive(Debug)]
pub(crate) struct Permit {
    state: PermitState,
}

pub(crate) struct Acquire<'a> {
    node: linked_list::Entry<Waiter>,
    semaphore: &'a Semaphore,
    permit: &'a mut Permit,
    num_permits: u16,
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

        self.add_permits_locked(0, true, self.waiters.lock().unwrap());
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

        self.add_permits_locked(n, false, self.waiters.lock().unwrap());
    }

    fn add_permits_locked(
        &self,
        mut rem: usize,
        mut closed: bool,
        mut waiters: MutexGuard<'_, LinkedList<Waiter>>,
    ) {
        while rem > 0 || closed {
            // how many permits are we releasing on this pass?

            let initial = rem;
            // Release the permits and notify
            while dbg!(rem > 0) {
                let pop = match waiters.last() {
                    Some(last) => dbg!(last.assign_permits(&mut rem, closed)),
                    None => {
                        self.permits.fetch_add(rem, Ordering::Release);
                        break;
                        // false
                    }
                };
                if pop {
                    waiters.pop_back().unwrap();
                }
            }

            let n = (initial - rem) << 1;

            let actual = if closed {
                let actual = self.add_lock.fetch_sub(n | 1, Ordering::AcqRel);
                closed = false;
                actual
            } else {
                let actual = self.add_lock.fetch_sub(n, Ordering::AcqRel);
                closed = actual & 1 == 1;
                actual
            };

            rem = (actual - initial) >> 1;
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        needed: u16,
        node: Pin<&mut linked_list::Entry<Waiter>>,
    ) -> Poll<Result<(), AcquireError>> {
        let mut curr = self.permits.load(Ordering::Acquire);
        let needed = needed as usize;
        let (acquired, remaining) = loop {
            dbg!(needed, curr);
            if curr == CLOSED {
                return Ready(Err(AcquireError(())));
            }
            let mut remaining = 0;
            let (next, acquired) = if dbg!(curr) >= dbg!(needed) {
                (curr - needed, needed)
            } else {
                remaining = needed - curr;
                (0, curr)
            };
            match self.permits.compare_exchange_weak(
                curr,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break (acquired, remaining),
                Err(actual) => curr = actual,
            }
        };
        dbg!(acquired, remaining);
        if remaining == 0 {
            return Ready(Ok(()));
        }

        assert!(node.is_unlinked());
        let waiter = unsafe { &*node.get() };
        waiter
            .state
            .compare_exchange(
                Waiter::UNQUEUED,
                remaining,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .expect("not unqueued");
        // otherwise, register the waker & enqueue the node.
        waiter.waker.register_by_ref(cx.waker());

        let mut queue = self.waiters.lock().unwrap();
        unsafe {
            queue.push_front(node);
            println!("enqueue");
        }

        Pending
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        self.close();
    }
}

impl fmt::Debug for Semaphore {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Semaphore")
            .field("permits", &self.add_lock.load(Ordering::Relaxed))
            .field("add_lock", &self.add_lock.load(Ordering::Relaxed))
            .finish()
    }
}

impl Permit {
    /// Creates a new `Permit`.
    ///
    /// The permit begins in the "unacquired" state.
    pub(crate) fn new() -> Permit {
        use PermitState::Acquired;

        Permit { state: Acquired(0) }
    }

    /// Returns `true` if the permit has been acquired
    pub(crate) fn is_acquired(&self) -> bool {
        match self.state {
            PermitState::Acquired(num) if num > 0 => true,
            _ => false,
        }
    }

    /// Returns a future that tries to acquire the permit. If no permits are available, the current task
    /// is notified once a new permit becomes available.
    pub(crate) fn acquire<'a>(
        &'a mut self,
        num_permits: u16,
        semaphore: &'a Semaphore,
    ) -> Acquire<'a> {
        self.state = match self.state {
            PermitState::Acquired(0) => PermitState::Waiting(num_permits),
            PermitState::Waiting(n) => PermitState::Waiting(cmp::max(n, num_permits)),
            PermitState::Acquired(n) => PermitState::Acquired(n),
        };
        Acquire {
            node: linked_list::Entry::new(Waiter::new()),
            semaphore,
            permit: self,
            num_permits,
        }
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
                panic!(
                    "cannot forget permits while in wait queue; we are already borrowed mutably?"
                )
                // let n = cmp::min(n, requested);
                // let node = unsafe { &*self.node.get() };
                // // Decrement
                // let acquired = node.try_dec_permits_to_acquire(n as usize) as u16;

                // if n == requested {
                //     self.state = Acquired(0);
                // // // TODO: rm from wait list here!
                // // semaphore
                // } else if acquired == requested - n {
                //     self.state = Waiting(acquired);
                // } else {
                //     self.state = Waiting(requested - n);
                // }

                // acquired
            }
            Acquired(acquired) => {
                let n = cmp::min(n, acquired);
                self.state = Acquired(acquired - n);
                dbg!(n)
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
    const UNQUEUED: usize = 1 << 16;
    fn new() -> Self {
        Waiter {
            waker: AtomicWaker::new(),
            state: AtomicUsize::new(Self::UNQUEUED),
        }
    }

    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize, closed: bool) -> bool {
        let mut curr = self.state.load(Ordering::Acquire);

        loop {
            // Number of permits to assign to this waiter
            let assign = cmp::min(curr, *n);
            let next = curr - assign;
            match self
                .state
                .compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire)
            {
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
            // if !curr.is_queued() {
            //     assert_eq!(0, curr.permits_to_acquire());
            // }

            let delta = cmp::min(n, curr);
            let rem = curr - delta;

            match self
                .state
                .compare_exchange(curr, rem, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return n - delta,
                Err(actual) => curr = actual,
            }
        }
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, permit, mut needed) = self.project();
        dbg!(&semaphore, &permit, &needed);
        permit.state = match permit.state {
            PermitState::Acquired(n) if n >= needed => return Ready(Ok(())),
            PermitState::Acquired(n) => {
                ready!(semaphore.poll_acquire(cx, needed - n, node))?;
                PermitState::Acquired(needed)
            }
            PermitState::Waiting(_n) => {
                assert_eq!(_n, needed, "how the heck did you get in this state?");
                if unsafe { &*node.get() }.state.load(Ordering::Acquire) > 0 {
                    ready!(semaphore.poll_acquire(cx, needed, node))?;
                }
                PermitState::Acquired(needed)
            }
        };
        Ready(Ok(()))
    }
}

impl Acquire<'_> {
    fn project(
        self: Pin<&mut Self>,
    ) -> (
        Pin<&mut linked_list::Entry<Waiter>>,
        &Semaphore,
        &mut Permit,
        u16,
    ) {
        fn is_unpin<T: Unpin>() {}
        unsafe {
            // Safety: all fields other than `node` are `Unpin`

            is_unpin::<&Semaphore>();
            is_unpin::<&mut Permit>();
            is_unpin::<u16>();

            let this = self.get_unchecked_mut();
            (
                Pin::new_unchecked(&mut this.node),
                &this.semaphore,
                &mut this.permit,
                this.num_permits,
            )
        }
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        dbg!("drop acquire");
        if dbg!(self.node.is_unlinked()) {
            // don't need to release permits
            return;
        }

        // Safety: if the node is linked, then we are already pinned
        let (mut node, semaphore, permit, needed) = unsafe { Pin::new_unchecked(self).project() };

        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = semaphore.waiters.lock().unwrap();

        // remove the entry from the list
        //
        // safety: the waiter is only added to `waiters` by virtue of it
        // being the only `LinkedList` available to the type.
        unsafe { waiters.remove(node.as_mut()) };

        // TODO(eliza): release permits to next waiter
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
