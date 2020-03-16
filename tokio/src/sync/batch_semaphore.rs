//! # Implementation Details
//! 
//! The semaphore is implemented using an intrusive linked list of waiters. An atomic counter
//! tracks the number of available permits. If the semaphore does not contain the required number
//! of permits, the task attempting to acquire permits places its waker at the end of a queue. When
//! new permits are made available (such as by releasing an initial acquisition), they are assigned
//! to the task at the front of the queue, waking that task if its requested number of permits is
//! met. 
//! 
//! Because waiters are enqueued at the back of the linked list and dequeued from the front, the
//! semaphore is fair. Tasks trying to acquire large numbers of permits at a time will always be
//! woken eventually, even if many other tasks are acquiring smaller numbers of permits. This means
//! that in a use-case like tokio's read-write lock, writers will not be starved by readers.
//! 
//! The linked list is guarded by a mutex, which must be acquired before enqueueing or dequeueing a
//! task. However, some operations are always wait-free.
use crate::loom::{
    cell::CausalCell,
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
        Waker,
    },
};

/// An asynchronous counting semaphore which permits waiting on multiple permits at once.
pub(crate) struct Semaphore {
    waiters: Mutex<Waitlist>,
    /// The current number of available permits in the semaphore.
    permits: AtomicUsize,

    /// Permits in the process of being released.
    /// 
    /// When releasing permits, if the lock on the semaphore's wait list is held by another task,
    /// the task releasing permits adds them to this counter. The task holding the lock will
    /// continue releasing permits until the counter is drained, allowing the `release` operation
    /// to be wait free.
    /// 
    /// The first bit of this counter indicates that the semaphore is closing. Therefore, all
    /// values are shifted over one bit.
    adding: AtomicUsize,
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

/// An entry in the wait queue.
struct Waiter {
    /// The current state of the waiter. 
    /// 
    /// This is either the number of remaining permits required by
    /// the waiter, or a flag indicating that the waiter is not yet queued.
    state: AtomicUsize,

    /// The waker to notify the task awaiting permits.
    /// 
    /// # Safety
    /// 
    /// This may only be accessed while the wait queue is locked.
    /// XXX: it would be nice if we could enforce this...
    waker: CausalCell<Option<Waker>>,

    /// Intrusive linked-list pointers.
    /// 
    /// # Safety
    /// 
    /// This may only be accessed while the wait queue is locked.
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
            adding: AtomicUsize::new(0),
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(Ordering::SeqCst) & std::u16::MAX as usize
    }


    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, added: usize) {
        // Assigning permits to waiters requires locking the wait list, so that
        // any waiters which receive their required number of permits can be
        // dequeued and notified. However, if multiple threads attempt to add
        // permits concurrently, we are able to make this operation wait-free.
        // By tracking an atomic counter of permits added to the semaphore, we
        // are able to determine if another thread is currently holding the lock
        // to assign permits. If this is the case, we simply add our permits to
        // the counter and return, so we don't need to acquire the lock.
        // Otherwise, no other thread is adding permits, so we lock the wait and
        // loop until the counter of added permits is drained.

        if added == 0 {
            return;
        }

        // TODO: Handle overflow. A panic is not sufficient, the process must
        // abort.
        let prev = self.adding.fetch_add(added << 1, Ordering::AcqRel);
        if prev > 0 {
            // Another thread is already assigning permits. It will continue to
            // do so until `added` is drained, so we don't need to block on the
            // lock.
            return;
        }

        // Otherwise, no other thread is assigning permits. Thus, we must lock
        // the semaphore's wait list and assign permits until `added` is
        // drained.
        self.add_permits_locked(added, self.waiters.lock().unwrap(), false);
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        // Closing the semaphore works similarly to adding permits: if another
        // thread is already holding the lock on the wait list to assign
        // permits, we simply set a bit in the counter of permits being added.
        // The thread holding the lock will see that bit has been set, and begin
        // closing the semaphore.

        self.permits.fetch_or(CLOSED, Ordering::Release);
        // Acquire the `added`, setting the "closed" flag on the lock.
        let prev = self.adding.fetch_or(1, Ordering::AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        let mut lock = self.waiters.lock().unwrap();
        lock.closed = true;
        self.add_permits_locked(0, lock, true);
    }

    fn add_permits_locked(
        &self,
        mut rem: usize,
        mut waiters: MutexGuard<'_, Waitlist>,
        mut closed: bool,
    ) {
        // The thread adding permits will loop until the counter of added
        // permits is drained. If threads add permits (or close the semaphore)
        // while we are holding the lock, we will add those permits as well, so
        // that the other thread does not need to block.

        loop {
            // How many permits are we releasing on this iteration?
            let initial = rem;
            // Assign permits to the wait queue and notify any satisfied
            // waiters.
            while rem > 0 || waiters.closed {
                let pop = match waiters.queue.last() {
                    Some(_) if waiters.closed => true,
                    Some(last) => last.assign_permits(&mut rem, waiters.closed),
                    None => {
                        self.permits.fetch_add(rem, Ordering::AcqRel);
                        break;
                    }
                };
                if pop {
                    let node = waiters.queue.pop_back().unwrap();
                    // Safety: we are holding the lock on the wait queue, and
                    // thus have exclusive access to the nodes' wakers.
                    let waker = unsafe { 
                        node.as_ref().waker.with_mut(|waker| (*waker).take())
                    };

                    waker.expect("if a node is in the wait list, it must have a waker").wake();

                }
            }

            let n = initial << 1;

            let actual = if closed {
                let actual = self.adding.fetch_sub(n | 1, Ordering::AcqRel);
                assert!(actual <= CLOSED, "permits to add overflowed! {}", actual);
                closed = false;
                actual >> 1
            } else {
                let actual = self.adding.fetch_sub(n, Ordering::AcqRel);
                assert!(actual <= CLOSED, "permits to add overflowed! {}", actual);
                if actual & 1 == 1 {
                    waiters.closed = true;
                    closed = true;
                }
                actual >> 1
            };

            rem = actual - initial;

            if rem == 0 && !closed {
                break;
            }
        }
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

        // First, try to take the requested number of permits from the
        // semaphore.
        let mut lock = None;
        let mut curr = self.permits.load(Ordering::Acquire);
        let mut waiters = loop {
            state = node.state.load(Ordering::Acquire);
            // Has the waiter already acquired all its needed permits? If so,
            // we're done!
            if state == 0 {
                return Ready(Ok(()));
            }

            // Has the semaphore closed?
            if curr & CLOSED > 0 {
                return Ready(Err(AcquireError::closed()));
            }

            let needed = cmp::min(state, needed as usize);
            let mut remaining = 0;
            let (next, acq) = if curr + acquired >= needed {
                let next = curr - (needed - acquired);
                (next, needed)
            } else {
                remaining = (needed - acquired) - curr;
                (0, curr)
            };

            if remaining > 0 && lock.is_none() {
                // No permits were immediately available, so this permit will
                // (probably) need to wait. We'll need to acquire a lock on the
                // wait queue.
                lock = Some(self.waiters.lock().unwrap());
                // While we were waiting to lock the wait list, additional
                // permits may have been released. Therefore, we will acquire a
                // new snapshot of the current state of the semaphore before
                // actually enqueueing the waiter..
                continue;
            }

            match self.permits.compare_exchange_weak(
                curr,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    acquired += acq;
                    if remaining == 0 {
                        return Ready(Ok(()));
                    }
                    break lock.unwrap();
                }
                Err(actual) => curr = actual,
            }
            drop(lock.take());
        };

        if waiters.closed {
            return Ready(Err(AcquireError(())));
        }

        let next = match acquired.cmp(&state) {
            // if the waiter is in the unqueued state, we need all the requested
            // permits, minus the amount we just acquired.
            _ if state >= Waiter::UNQUEUED => needed as usize - acquired,
            // We have acquired all the needed permits!
            cmp::Ordering::Equal => 0,
            cmp::Ordering::Less => state - acquired,
            cmp::Ordering::Greater => {
                unreachable!("cannot have been assigned more than needed permits")
            }
        };
        
        // Typically, one would probably expect to see a compare-and-swap loop
        // here. However, in this case, we don't need to loop. Our most snapshot
        // of the node's current state was acquired after we acquired the lock
        // on the wait list. Because releasing permits tPPo waiters also
        // requires locking the wait list, the state cannot have changed since
        // we acquired this snapshot.
        node.state
            .compare_exchange(state, next, Ordering::AcqRel, Ordering::Acquire)
            .expect("state cannot have changed while the wait list was locked");

        if next == 0 {
            return Ready(Ok(()));
        }

        // otherwise, register the waker & enqueue the node.
        node.waker.with_mut(|waker| 
            // safety: the wait list is locked, so we may modify the waker.
            unsafe { *waker = Some(cx.waker().clone()) }
        );

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
            .field("permits", &self.adding.load(Ordering::Relaxed))
            .field("added", &self.adding.load(Ordering::Relaxed))
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

    /// Returns a future that tries to acquire the permit. If no permits are
    /// available, the current task is notified once a new permit becomes
    /// available.
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
    const UNQUEUED: usize = 1 << 16;

    fn new() -> Self {
        Waiter {
            waker: CausalCell::new(None),
            state: AtomicUsize::new(Self::UNQUEUED),
            pointers: linked_list::Pointers::new(),
            _p: PhantomPinned,
        }
    }

    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize, closed: bool) -> bool {
        // If the wait list has closed, consume no permits but pop the node.
        if closed {
            return true;
        }

        let mut curr = self.state.load(Ordering::Acquire);

        loop {
            // Assign up to `n` permits.
            let assign = cmp::min(curr, *n);
            let next = curr - assign;
            match self
                .state
                .compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    *n = n.saturating_sub(assign);

                    // If we have assigned all remaining permits, return true.
                    return next == 0;
                }
                Err(actual) => curr = actual,
            }
        }
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, permit, needed) = self.project();
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
        let state = self.node.state.load(Ordering::Acquire);
        // fast path: if we aren't actually waiting, no need to acquire the lock.
        if state & Waiter::UNQUEUED == Waiter::UNQUEUED {
            return;
        }
        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = self.semaphore.waiters.lock().unwrap();
        let node = NonNull::from(&mut self.node);
        if waiters.queue.is_linked(&node) {
    
            let acquired_permits = self.num_permits as usize - state;
            // remove the entry from the list
            //
            // safety: we have locked the wait list.
            unsafe { waiters.queue.remove(node) };

            if acquired_permits > 0 {
                // we have already locked the mutex, so we know we will be the
                // one to release these permits, but it's necessary to add them
                // since we will try to subtract them once we have finished
                // permit assignment.
                self.semaphore.adding.fetch_add(acquired_permits << 1, Ordering::AcqRel);
                self.semaphore.add_permits_locked(acquired_permits, waiters, false);
            }

        } else {
            drop(waiters);
        };

        // If the permit had transitioned to the `Waiting` state, put it back into `Acquired`.
        if let PermitState::Waiting(_) = self.permit.state {
            self.permit.state = PermitState::Acquired(0);
        }
    }
}

// ===== impl AcquireError ====

impl AcquireError {
    fn closed() -> AcquireError {
        AcquireError(())
    }
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
    #[allow(dead_code)] // may be used later!
    pub(crate) fn is_closed(&self) -> bool {
        match self {
            TryAcquireError::Closed => true,
            _ => false,
        }
    }

    /// Returns `true` if the error was caused by calling `try_acquire` on a
    /// semaphore with no available permits.
    #[allow(dead_code)] // may be used later!
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
    // XXX: ideally, we would be able to use `Pin` here, to enforce the invariant that list entries
    // may not move while in the list. However, we can't do this currently, as using `Pin<&'a mut
    // Waiter>` as the `Handle` type would require `Semaphore` to be generic over a lifetime. We
    // can't use `Pin<*mut Waiter>`, as raw pointers are `Unpin` regardless of whether or not they
    // dereference to an `!Unpin` target.
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
