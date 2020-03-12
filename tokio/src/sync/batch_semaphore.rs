use crate::loom::{
    future::AtomicWaker,
    sync::{atomic::AtomicUsize, Mutex, MutexGuard},
};
use crate::util::linked_list::{self, LinkedList};

use std::{
    cmp, fmt,
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::Ordering,
    task::{
        Context, Poll,
        Poll::{Pending, Ready},
    },
};

macro_rules! ddbg {
    ($x:expr) => {
        dbg!($x)
    };
    ($($x:expr),+) => {};
}

pub(crate) struct Semaphore {
    waiters: Mutex<Waitlist>,
    permits: AtomicUsize,
    add_lock: AtomicUsize,
}

struct Waitlist {
    queue: LinkedList<Waiter>,
    closed: bool,
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
    node: Waiter,
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

    /// Intrusive linked-list pointers
    pointers: linked_list::Pointers<Waiter>,

    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

const CLOSED: usize = 1 << 17;

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    pub(crate) fn new(permits: usize) -> Self {
        assert!(permits <= std::u16::MAX as usize);
        Self {
            permits: AtomicUsize::new(permits),
            waiters: Mutex::new(Waitlist {
                queue: LinkedList::new(),
                closed: false,
            }),
            add_lock: AtomicUsize::new(0),
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire) & std::u16::MAX as usize
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        dbg!("closing...");
        self.permits.fetch_or(CLOSED, Ordering::Release);
        // Acquire the `add_lock`, setting the "closed" flag on the lock.
        let prev = self.add_lock.fetch_or(1, Ordering::AcqRel);
        dbg!("closed");

        if dbg!(prev) != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }
        let mut lock = self.waiters.lock().unwrap();
        lock.closed = true;
        println!("locked");
        self.add_permits_locked(0, lock, true);
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, added: usize) {
        ddbg!(added);
        if added == 0 {
            return;
        }

        // TODO: Handle overflow. A panic is not sufficient, the process must
        // abort.
        let prev = self.add_lock.fetch_add(added << 1, Ordering::AcqRel);
        if prev > 0 {
            return;
        }

        self.add_permits_locked(added, self.waiters.lock().unwrap(), false);
    }

    fn add_permits_locked(
        &self,
        mut rem: usize,
        mut waiters: MutexGuard<'_, Waitlist>,
        mut closed: bool,
    ) {
        println!(" ADD PERMITS LOCKED ");

        loop {
            // how many permits are we releasing on this pass?
            let initial = rem;
            // Release the permits and notify
            while dbg!(rem) > 0 || dbg!(waiters.closed) {
                let pop = match waiters.queue.last() {
                    Some(_) if waiters.closed => true,
                    Some(last) => {
                        ddbg!(format_args!("assign permits to {:p}", last));
                        let res = ddbg!(last.assign_permits(&mut rem, waiters.closed));
                        // dbg!(last.is_unlinked());
                        res
                    }
                    None => {
                        dbg!("queue empty");
                        let _prev = self.permits.fetch_add(rem, Ordering::AcqRel);
                        dbg!(_prev + rem);
                        break;
                        // false
                    }
                };
                if pop {
                    dbg!("popping");
                    let waiter = waiters.queue.pop_back().unwrap();
                    dbg!(format_args!("popped {:?}", waiter));
                    unsafe {
                        waiter.as_ref().waker.wake();
                    }
                    dbg!("woke");
                }
            }

            let n = initial << 1;

            dbg!(n);
            let actual = if closed {
                let actual = dbg!(self.add_lock.fetch_sub(n | 1, Ordering::AcqRel));
                assert!(actual < std::usize::MAX);
                closed = false;
                actual >> 1
            } else {
                let actual = dbg!(self.add_lock.fetch_sub(n, Ordering::AcqRel));
                assert!(actual < std::usize::MAX);
                if actual & 1 == 1 {
                    waiters.closed = true;
                    closed = true;
                }
                actual >> 1
            };

            rem = actual - initial;

            if dbg!(rem) == 0 && !dbg!(closed) {
                break;
            }
        }

        println!("DONE ADDING PERMITS");
    }

    fn try_acquire(&self, num_permits: u16) -> Result<(), TryAcquireError> {
        let mut curr = self.permits.load(Ordering::Acquire);
        loop {
            // Has the semaphore closed?
            if curr & CLOSED > 0 {
                return Err(TryAcquireError::Closed);
            }

            // Are there enough permits remaining?
            if (curr as u16) < num_permits {
                return Err(TryAcquireError::NoPermits);
            }

            let next = curr - num_permits as usize;

            match self
                .permits
                .compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Ok(()),
                Err(actual) => curr = actual,
            }
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        needed: u16,
        node: Pin<&mut Waiter>,
    ) -> Poll<Result<(), AcquireError>> {
        let mut state;
        let mut acquired = 0;

        // First, try to take the requested number of permits from the semaphore.
        let mut lock = None;
        let mut curr = self.permits.load(Ordering::Acquire);
        let waiters = loop {
            state = dbg!(node.state.load(Ordering::Acquire));

            // Has the waiter already acquired all its needed permits? If so, we're done!
            if state == 0 {
                return Ready(Ok(()));
            }

            // Has the semaphore closed?
            if curr & CLOSED > 0 {
                return Ready(Err(AcquireError(())));
            }

            let needed = cmp::min(state, needed as usize);
            ddbg!(needed, curr);
            let mut remaining = 0;
            let (next, acq) = if ddbg!(curr + acquired) >= ddbg!(needed) {
                let next = curr - (needed - acquired);
                (dbg!(next), needed)
            } else {
                remaining = (needed - acquired) - curr;
                (0, curr)
            };

            if remaining > 0 && lock.is_none() {
                // No permits were immediately available, so this permit will (probably) need to wait.
                // We'll need to acquire a lock on the wait queue.
                // Otherwise, the wait list is currently locked. In that case, we'll need to
                // check the available permits a second time before we enqueue the node to wait
                // --- someone else might be in the process of releasing permits.
                lock = Some(self.waiters.lock().unwrap());
                continue;
            }

            match self.permits.compare_exchange_weak(
                curr,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    ddbg!(acquired, remaining);
                    acquired += acq;
                    if remaining == 0 {
                        return Ready(Ok(()));
                    }
                    break lock;
                }
                Err(actual) => curr = actual,
            }
        };
        let mut waiters = waiters.unwrap();

        dbg!("LOCKED");
        if dbg!(waiters.closed) {
            return Ready(Err(AcquireError(())));
        }

        let mut next;
        loop {
            // were we assigned permits while blocking on the lock?
            next = if dbg!(state >= Waiter::UNQUEUED) {
                dbg!(needed as usize - acquired)
            } else if acquired > state {
                0
            } else {
                state - acquired
            };
            match node.state.compare_exchange(
                dbg!(state),
                dbg!(next),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => state = actual,
            }
        }

        if next & std::u16::MAX as usize == 0 {
            return Ready(Ok(()));
        }

        // otherwise, register the waker & enqueue the node.
        node.waker.register_by_ref(cx.waker());

        unsafe {
            // XXX(eliza) T_T
            let node = Pin::into_inner_unchecked(node) as *mut _;
            let node = NonNull::new_unchecked(node);

            if !waiters.queue.is_linked(&node) {
                waiters.queue.push_front(node);
            }
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
            node: Waiter::new(),
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
        use PermitState::*;

        match self.state {
            Waiting(_) => unreachable!(
                "if a permit is waiting, then it has been borrowed mutably by an `Acquire` future"
            ),
            Acquired(acquired) => {
                if acquired < num_permits {
                    semaphore.try_acquire(num_permits - acquired)?;
                    self.state = Acquired(num_permits);
                }
            }
        }

        Ok(())
    }

    /// Releases a permit back to the semaphore
    pub(crate) fn release(&mut self, n: u16, semaphore: &Semaphore) {
        let n = self.forget(n);
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
    pub(crate) fn forget(&mut self, n: u16) -> u16 {
        use PermitState::*;

        match self.state {
            Waiting(_) => unreachable!(
                "cannot forget permits while in wait queue; we are already borrowed mutably?"
            ),
            Acquired(acquired) => {
                let n = cmp::min(n, acquired);
                self.state = Acquired(acquired - n);
                ddbg!(n)
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
            pointers: linked_list::Pointers::new(),
            _p: PhantomPinned,
        }
    }

    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize, closed: bool) -> bool {
        if dbg!(closed) {
            return true;
        }
        let mut curr = self.state.load(Ordering::Acquire);
        dbg!(format_args!(
            "assigning {} permits to {:p} (curr: {})",
            n, self, curr
        ));

        loop {
            // Number of permits to assign to this waiter
            let assign = cmp::min(curr, *n);
            let next = curr - assign;
            let next = if closed { next | CLOSED } else { next };
            match self
                .state
                .compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // Update `n`
                    *n = n.saturating_sub(assign);

                    return next == 0;
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
        let (node, semaphore, permit, needed) = self.project();
        ddbg!(&semaphore, &permit, &needed);
        permit.state = match permit.state {
            PermitState::Acquired(n) if n >= needed => return Ready(Ok(())),
            PermitState::Acquired(n) => {
                ready!(semaphore.poll_acquire(cx, needed - n, node))?;
                PermitState::Acquired(needed)
            }
            PermitState::Waiting(_n) => {
                assert_eq!(_n, needed, "how the heck did you get in this state?");
                if node.state.load(Ordering::Acquire) > 0 {
                    ready!(semaphore.poll_acquire(cx, needed, node))?;
                }
                PermitState::Acquired(needed)
            }
        };
        Ready(Ok(()))
    }
}

impl Acquire<'_> {
    fn project(self: Pin<&mut Self>) -> (Pin<&mut Waiter>, &Semaphore, &mut Permit, u16) {
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
        ddbg!(format_args!("drop acquire {:p}", self));

        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = self.semaphore.waiters.lock().unwrap();
        let node = NonNull::from(&mut self.node);
        if ddbg!(!waiters.queue.is_linked(&node)) {
            // don't need to release permits
            return;
        }
        // remove the entry from the list
        //
        // safety: the waiter is only added to `waiters` by virtue of it
        // being the only `LinkedList` available to the type.
        unsafe { waiters.queue.remove(node) };

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

/// # Safety
///
/// `Waiter` is forced to be !Unpin.
unsafe impl linked_list::Link for Waiter {
    type Handle = NonNull<Waiter>;
    type Target = Waiter;

    fn as_raw(handle: &Self::Handle) -> NonNull<Waiter> {
        *handle
    }

    unsafe fn from_raw(ptr: NonNull<Waiter>) -> NonNull<Waiter> {
        ptr
    }

    unsafe fn pointers(mut target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        NonNull::from(&mut target.as_mut().pointers)
    }
}
