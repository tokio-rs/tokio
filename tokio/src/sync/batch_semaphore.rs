//! # Implementation Details
//!
//! The semaphore is implemented using an intrusive linked list of waiters. An
//! atomic counter tracks the number of available permits. If the semaphore does
//! not contain the required number of permits, the task attempting to acquire
//! permits places its waker at the end of a queue. When new permits are made
//! available (such as by releasing an initial acquisition), they are assigned
//! to the task at the front of the queue, waking that task if its requested
//! number of permits is met. 
//!
//! Because waiters are enqueued at the back of the linked list and dequeued
//! from the front, the semaphore is fair. Tasks trying to acquire large numbers
//! of permits at a time will always be woken eventually, even if many other
//! tasks are acquiring smaller numbers of permits. This means that in a
//! use-case like tokio's read-write lock, writers will not be starved by
//! readers.
use crate::loom::cell::CausalCell;
use crate::loom::sync::{atomic::AtomicUsize, Mutex, MutexGuard};
use crate::util::linked_list::{self, LinkedList};

use std::{cmp, fmt};
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::*;
use std::task::{Context, Poll, Waker};
use std::task::Poll::*;

/// An asynchronous counting semaphore which permits waiting on multiple permits at once.
pub(crate) struct Semaphore {
    waiters: Mutex<Waitlist>,
    /// The current number of available permits in the semaphore.
    permits: AtomicUsize,
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
    Waiting(u16, u16),

    /// The number of acquired permits
    Acquired(u16),
}

/// An entry in the wait queue.
struct Waiter {
    /// The current state of the waiter. 
    /// 
    /// This is either the number of remaining permits required by
    /// the waiter, or a flag indicating that the waiter is not yet queued.
    state: CausalCell<usize>,

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

fn notify_all(list: &mut LinkedList<Waiter>) {
    while let Some(waiter) = list.pop_back() {
        let waker = unsafe { 
            waiter.as_ref().waker.with_mut(|waker| (*waker).take())
        };

        waker.expect("if a node is in the wait list, it must have a waker").wake();
    }
}

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
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(SeqCst) & std::u16::MAX as usize
    }


    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, added: usize) {
        if added == 0 {
            return;
        }

        // Assign permits to the wait queue, returning a list containing all the
        // waiters at the back of the queue that received enough permits to wake
        // up.
        let mut notified = self.add_permits_locked(added, self.waiters.lock().unwrap());
        
        // Once we release the lock, notify all woken waiters.
        notify_all(&mut notified);

    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        self.permits.fetch_or(CLOSED, Release);

        let mut waiters = self.waiters.lock().unwrap();
        waiters.closed = true;
        notify_all(&mut waiters.queue)
    }

    fn add_permits_locked(
        &self,
        mut rem: usize,
        mut waiters: MutexGuard<'_, Waitlist>,
    ) -> LinkedList<Waiter> {
        // Starting from the back of the wait queue, assign each waiter as many
        // permits as it needs until we run out of permits to assign.
        let mut last = None;
        for waiter in waiters.queue.iter().rev() {
            if waiter.assign_permits(&mut rem) {
                last = Some(NonNull::from(waiter));
            } else {
                break;
            }
        }

        // If we assigned permits to all the waiters in the queue, and there are
        // still permits left over, assign them back to the semaphore.
        if rem > 0 {
            self.permits.fetch_add(rem, Release);
        }

        // Split off the queue at the last waiter that was satisfied, creating a
        // new list. Once we release the lock, we'll drain this list and notify
        // the waiters in it.
        if let Some(waiter) = last {
            // Safety: it's only safe to call `split_back` with a pointer to a
            // node in the same list as the one we call `split_back` on. Since 
            // we got the waiter pointer from the list's iterator, this is fine.
            unsafe {
                waiters.queue.split_back(waiter)
            }
        } else {
            LinkedList::new()
        }
    }

    fn try_acquire(&self, num_permits: u16) -> Result<(), TryAcquireError> {
        let mut curr = self.permits.load(Acquire);
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
                .compare_exchange(curr, next, AcqRel, Acquire)
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
        queued: bool,
    ) -> Poll<Result<(), AcquireError>> {
        let mut acquired = 0;

        let (needed, mut lock) = if queued {
            let lock = self.waiters.lock().unwrap();
            let needed = node.state.with(|curr| unsafe {*curr});
            (needed, Some(lock))
        } else {
            (needed as usize, None)
        };
        // First, try to take the requested number of permits from the
        // semaphore.
        let mut curr = self.permits.load(Acquire);
        let mut waiters = loop {
            // state = node.state.load(Acquire);
            // // Has the waiter already acquired all its needed permits? If so,
            // // we're done!
            // if state == 0 {
            //     return Ready(Ok(()));
            // }

            // Has the semaphore closed?
            if curr & CLOSED > 0 {
                return Ready(Err(AcquireError::closed()));
            }
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
                curr = self.permits.load(Acquire);
                continue;
            }

            match self.permits.compare_exchange_weak(
                curr,
                next,
                AcqRel,
                Acquire,
            ) {
                Ok(_) => {
                    acquired += acq;
                    if remaining == 0 && !queued {
                        return Ready(Ok(()));
                    }
                    break lock.unwrap();
                }
                Err(actual) => curr = actual,
            }
            drop(lock.take());
        };

        node.state.with_mut(|curr| {
            let curr = unsafe { &mut *curr };
            if *curr & Waiter::UNQUEUED == Waiter::UNQUEUED {
                *curr = needed;
            }
        });

        if dbg!(node.assign_permits(&mut acquired)) {
            if dbg!(acquired) > 0 {
                // We ended up with more permits than we needed. Release the
                // back to the semaphore.
                self.add_permits_locked(acquired, waiters);
            }
            return Ready(Ok(()));
        }

        assert_eq!(acquired, 0);

        // Otherwise, register the waker & enqueue the node.
        node.waker.with_mut(|waker| 
            // Safety: the wait list is locked, so we may modify the waker.
            unsafe { *waker = Some(cx.waker().clone()) }
        );

        unsafe {
            let node = Pin::into_inner_unchecked(node) as *mut _;
            let node = NonNull::new_unchecked(node);

            // If the waiter is not already in the wait queue, enqueue it.
            if !waiters.contains(node) {
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
            .field("permits", &self.permits.load(Relaxed))
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
        mut num_permits: u16,
        semaphore: &'a Semaphore,
    ) -> Acquire<'a> {
        self.state = match self.state {
            PermitState::Acquired(0) => PermitState::Waiting(num_permits, 0),
            PermitState::Waiting(n, n2) => {
                num_permits = cmp::max(n, num_permits - n2);
                PermitState::Waiting(num_permits, n2)
            },
            PermitState::Acquired(n) => {
                PermitState::Acquired(n)
            },
        };
        Acquire {
            node: Waiter::new(num_permits),
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
            Waiting(_, _) => unreachable!(
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
            Waiting(_, _) => unreachable!(
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

    fn new(num_permits: u16) -> Self {
        Waiter {
            waker: CausalCell::new(None),
            state: CausalCell::new(num_permits as usize),
            pointers: linked_list::Pointers::new(),
            _p: PhantomPinned,
        }
    }

    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize) -> bool {
        self.state.with_mut(|curr| {
            let curr = unsafe {&mut *curr};

            dbg!(*n, *curr);
            // Assign up to `n` permits.
            let assign = cmp::min(*curr, *n);
            *curr -= assign;
            *n -= assign;
            *curr == 0
        })
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, permit, needed) = self.project();
        let mut res = Pending;
        permit.state = match permit.state {
            PermitState::Acquired(n) if n >= needed => return Ready(Ok(())),
            PermitState::Acquired(n) => {
                match semaphore.poll_acquire(cx, needed - n, node, false) {
                    Ready(r) => { 
                        r?;
                        res = Ready(Ok(()));
                        PermitState::Acquired(needed)
                    },
                    Pending => {
                        PermitState::Waiting(needed, n)
                    },
                }
            }
            PermitState::Waiting(n, n2) => {
                assert_eq!(n + n2, needed, "permit cannot be modified while waiting");
                match semaphore.poll_acquire(cx, n - n2, node, true) {
                    Ready(r) => { 
                        r?;
                        res = Ready(Ok(()));
                        PermitState::Acquired(needed)
                    },
                    Pending => {
                        PermitState::Waiting(needed, n2)
                    },
                }
            }
        };
        res
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
        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = self.semaphore.waiters.lock().unwrap();
        let state = self.node.state.with(|curr| unsafe { *curr });

        // are we even waiting?
        if state & Waiter::UNQUEUED == Waiter::UNQUEUED {
            return;
        }

        let node = NonNull::from(&mut self.node);
        if waiters.contains(node) {
    
            let acquired_permits = self.num_permits as usize - state;
            // remove the entry from the list
            //
            // Safety: we have locked the wait list.
            unsafe { waiters.queue.remove(node) };

            if acquired_permits > 0 {
                let mut notified = self.semaphore.add_permits_locked(acquired_permits, waiters);
                notify_all(&mut notified);
            }

        } else {
            // We don't need to continue holding the lock while modifying the 
            // permit's local state.
            drop(waiters);
        }


        // If the permit had transitioned to the `Waiting` state, put it back
        // into `Acquired`.
        if let PermitState::Waiting(_, prior) = self.permit.state {
            self.permit.state = PermitState::Acquired(prior);
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
    // XXX: ideally, we would be able to use `Pin` here, to enforce the
    // invariant that list entries may not move while in the list. However, we
    // can't do this currently, as using `Pin<&'a mut Waiter>` as the `Handle`
    // type would require `Semaphore` to be generic over a lifetime. We can't
    // use `Pin<*mut Waiter>`, as raw pointers are `Unpin` regardless of whether
    // or not they dereference to an `!Unpin` target.
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

impl Waitlist {
    /// Returns true if this waitlist already contains the given waiter
    fn contains(&self, node: NonNull<Waiter>) -> bool {
        use linked_list::Link;
        // Note: `is_linked` does not necessarily indicate that the node is
        // linked with _this_ list. However, because nodes are only
        // added/removed inside of `Acquire` futures, and a reference to the
        // same `Semaphore` is present whenever the `Acquire` future calls this
        // we know that the node cannot be linked with another list.
        if unsafe { Waiter::pointers(node).as_ref() }.is_linked() {
            true
        } else if self.queue.is_first(&node) {
            debug_assert!(
                self.queue.is_last(&node), 
                "if a node is unlinked but is the head of a queue, it must \
                also be the tail of the queue"
            );
            true
        } else {
            false
        }
    }
}
