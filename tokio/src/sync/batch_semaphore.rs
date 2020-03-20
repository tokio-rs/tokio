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

use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::*;
use std::task::Poll::*;
use std::task::{Context, Poll, Waker};
use std::{cmp, fmt};

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
    num_permits: u16,
}

pub(crate) struct Acquire<'a> {
    node: Waiter,
    semaphore: &'a Semaphore,
    num_permits: u16,
    queued: bool,
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

fn notify_all(mut list: LinkedList<Waiter>) {
    while let Some(waiter) = list.pop_back() {
        let waker = unsafe { waiter.as_ref().waker.with_mut(|waker| (*waker).take()) };

        waker
            .expect("if a node is in the wait list, it must have a waker")
            .wake();
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
        self.permits.load(Acquire) & std::u16::MAX as usize
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, added: usize) {
        if added == 0 {
            return;
        }

        // Assign permits to the wait queue, returning a list containing all the
        // waiters at the back of the queue that received enough permits to wake
        // up.
        let notified = self.add_permits_locked(added, self.waiters.lock().unwrap());

        // Once we release the lock, notify all woken waiters.
        notify_all(notified);
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        self.permits.fetch_or(CLOSED, Release);

        let notified = {
            let mut waiters = self.waiters.lock().unwrap();
            waiters.closed = true;
            waiters.queue.take_all()
        };
        notify_all(notified)
    }

    pub(crate) fn try_acquire(&self, num_permits: u16) -> Result<Permit, TryAcquireError> {
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

            match self.permits.compare_exchange(curr, next, AcqRel, Acquire) {
                Ok(_) => return Ok(Permit { num_permits }),
                Err(actual) => curr = actual,
            }
        }
    }

    pub(crate) fn acquire(&self, num_permits: u16) -> Acquire<'_> {
        Acquire::new(self, num_permits)
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
            unsafe { waiters.queue.split_back(waiter) }
        } else {
            LinkedList::new()
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        num_permits: u16,
        node: Pin<&mut Waiter>,
        queued: bool,
    ) -> Poll<Result<Permit, AcquireError>> {
        let mut acquired = 0;

        // If we are already in the wait queue, we need to lock the queue so we
        // can access the wait queue entry's current state.
        let (needed, mut lock) = if queued {
            let lock = self.waiters.lock().unwrap();
            // Safety: since we have acquired the lock, it is safe to look at
            // the waiter's state.
            let needed = node.state.with(|curr| unsafe { *curr });
            (needed, Some(lock))
        } else {
            (num_permits as usize, None)
        };

        // First, try to take the requested number of permits from the
        // semaphore.
        let mut curr = self.permits.load(Acquire);
        let mut waiters = loop {
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
                // wait queue before continuing. We need to do this _before_ the
                // CAS that sets the new value of the semaphore's `permits`
                // counter. Otherwise, if we subtract the permits and then
                // acquire the lock, we might miss additional permits being
                // added while waiting for the lock.
                lock = Some(self.waiters.lock().unwrap());
            }

            match self
                .permits
                .compare_exchange_weak(curr, next, AcqRel, Acquire)
            {
                Ok(_) => {
                    acquired += acq;
                    if remaining == 0 && !queued {
                        return Ready(Ok(Permit { num_permits }));
                    }
                    break lock.unwrap();
                }
                Err(actual) => curr = actual,
            }
        };

        if node.assign_permits(&mut acquired) {
            if acquired > 0 {
                // We ended up with more permits than we needed. Release the
                // back to the semaphore.
                self.add_permits_locked(acquired, waiters);
            }
            return Ready(Ok(Permit { num_permits }));
        }

        assert_eq!(acquired, 0);

        // Otherwise, register the waker & enqueue the node.
        unsafe {
            node.waker.with_mut(|waker| {
                // Safety: the wait list is locked, so we may modify the waker.
                *waker = Some(cx.waker().clone());
            });

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
    /// Returns a future that tries to acquire the permit. If no permits are
    /// available, the current task is notified once a new permit becomes
    /// available.
    pub(crate) async fn acquire(
        &mut self,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Result<(), AcquireError> {
        if num_permits <= self.num_permits {
            return Ok(());
        }
        let num_permits = num_permits - self.num_permits;
        let acquired = semaphore.acquire(num_permits).await?;
        self.num_permits += acquired.num_permits;
        Ok(())
    }

    /// Tries to acquire the permit.
    pub(crate) fn try_acquire(
        &mut self,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Result<(), TryAcquireError> {
        if num_permits <= self.num_permits {
            return Ok(());
        }
        let num_permits = num_permits - self.num_permits;
        let acquired = semaphore.try_acquire(num_permits)?;
        self.num_permits += acquired.num_permits;
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
        let n = cmp::min(n, self.num_permits);
        self.num_permits -= n;
        n
    }
}

impl Waiter {
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
            let curr = unsafe { &mut *curr };
            // Assign up to `n` permits.
            let assign = cmp::min(*curr, *n);
            *curr -= assign;
            *n -= assign;
            *curr == 0
        })
    }
}

impl Future for Acquire<'_> {
    type Output = Result<Permit, AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, needed, queued) = self.project();
        match semaphore.poll_acquire(cx, needed, node, *queued) {
            Pending => {
                *queued = true;
                Pending
            }
            Ready(r) => {
                *queued = false;
                Ready(r)
            }
        }
    }
}

impl<'a> Acquire<'a> {
    fn new(semaphore: &'a Semaphore, num_permits: u16) -> Self {
        Self {
            node: Waiter::new(num_permits),
            semaphore,
            num_permits,
            queued: false,
        }
    }

    fn project(self: Pin<&mut Self>) -> (Pin<&mut Waiter>, &Semaphore, u16, &mut bool) {
        fn is_unpin<T: Unpin>() {}
        unsafe {
            // Safety: all fields other than `node` are `Unpin`

            is_unpin::<&Semaphore>();
            is_unpin::<&mut bool>();
            is_unpin::<u16>();

            let this = self.get_unchecked_mut();
            (
                Pin::new_unchecked(&mut this.node),
                &this.semaphore,
                this.num_permits,
                &mut this.queued,
            )
        }
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        // If the future is completed, there is no node in the wait list, so we
        // can skip acquiring the lock.
        if !self.queued {
            return;
        }

        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = self.semaphore.waiters.lock().unwrap();
        let state = self.node.state.with(|curr| unsafe { *curr });

        let node = NonNull::from(&mut self.node);
        if waiters.contains(node) {
            let acquired_permits = self.num_permits as usize - state;
            // remove the entry from the list
            //
            // Safety: we have locked the wait list.
            unsafe { waiters.queue.remove(node) };

            if acquired_permits > 0 {
                let notified = self.semaphore.add_permits_locked(acquired_permits, waiters);
                notify_all(notified);
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
