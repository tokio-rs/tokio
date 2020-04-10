#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

//! Thread-safe, asynchronous counting semaphore.
//!
//! A `Semaphore` instance holds a set of permits. Permits are used to
//! synchronize access to a shared resource.
//!
//! Before accessing the shared resource, callers acquire a permit from the
//! semaphore. Once the permit is acquired, the caller then enters the critical
//! section. If no permits are available, then acquiring the semaphore returns
//! `Pending`. The task is woken once a permit becomes available.

use crate::loom::cell::UnsafeCell;
use crate::loom::future::AtomicWaker;
use crate::loom::sync::atomic::{AtomicPtr, AtomicUsize};
use crate::loom::thread;

use std::cmp;
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{self, AcqRel, Acquire, Relaxed, Release};
use std::task::Poll::{Pending, Ready};
use std::task::{Context, Poll};
use std::usize;

/// Futures-aware semaphore.
pub(crate) struct Semaphore {
    /// Tracks both the waiter queue tail pointer and the number of remaining
    /// permits.
    state: AtomicUsize,

    /// waiter queue head pointer.
    head: UnsafeCell<NonNull<Waiter>>,

    /// Coordinates access to the queue head.
    rx_lock: AtomicUsize,

    /// Stub waiter node used as part of the MPSC channel algorithm.
    stub: Box<Waiter>,
}

/// A semaphore permit
///
/// Tracks the lifecycle of a semaphore permit.
///
/// An instance of `Permit` is intended to be used with a **single** instance of
/// `Semaphore`. Using a single instance of `Permit` with multiple semaphore
/// instances will result in unexpected behavior.
///
/// `Permit` does **not** release the permit back to the semaphore on drop. It
/// is the user's responsibility to ensure that `Permit::release` is called
/// before dropping the permit.
#[derive(Debug)]
pub(crate) struct Permit {
    waiter: Option<Box<Waiter>>,
    state: PermitState,
}

/// Error returned by `Permit::poll_acquire`.
#[derive(Debug)]
pub(crate) struct AcquireError(());

/// Error returned by `Permit::try_acquire`.
#[derive(Debug)]
pub(crate) enum TryAcquireError {
    Closed,
    NoPermits,
}

/// Node used to notify the semaphore waiter when permit is available.
#[derive(Debug)]
struct Waiter {
    /// Stores waiter state.
    ///
    /// See `WaiterState` for more details.
    state: AtomicUsize,

    /// Task to wake when a permit is made available.
    waker: AtomicWaker,

    /// Next pointer in the queue of waiting senders.
    next: AtomicPtr<Waiter>,
}

/// Semaphore state
///
/// The 2 low bits track the modes.
///
/// - Closed
/// - Full
///
/// When not full, the rest of the `usize` tracks the total number of messages
/// in the channel. When full, the rest of the `usize` is a pointer to the tail
/// of the "waiting senders" queue.
#[derive(Copy, Clone)]
struct SemState(usize);

/// Permit state
#[derive(Debug, Copy, Clone)]
enum PermitState {
    /// Currently waiting for permits to be made available and assigned to the
    /// waiter.
    Waiting(u16),

    /// The number of acquired permits
    Acquired(u16),
}

/// State for an individual waker node
#[derive(Debug, Copy, Clone)]
struct WaiterState(usize);

/// Waiter node is in the semaphore queue
const QUEUED: usize = 0b001;

/// Semaphore has been closed, no more permits will be issued.
const CLOSED: usize = 0b10;

/// The permit that owns the `Waiter` dropped.
const DROPPED: usize = 0b100;

/// Represents "one requested permit" in the waiter state
const PERMIT_ONE: usize = 0b1000;

/// Masks the waiter state to only contain bits tracking number of requested
/// permits.
const PERMIT_MASK: usize = usize::MAX - (PERMIT_ONE - 1);

/// How much to shift a permit count to pack it into the waker state
const PERMIT_SHIFT: u32 = PERMIT_ONE.trailing_zeros();

/// Flag differentiating between available permits and waiter pointers.
///
/// If we assume pointers are properly aligned, then the least significant bit
/// will always be zero. So, we use that bit to track if the value represents a
/// number.
const NUM_FLAG: usize = 0b01;

/// Signal the semaphore is closed
const CLOSED_FLAG: usize = 0b10;

/// Maximum number of permits a semaphore can manage
const MAX_PERMITS: usize = usize::MAX >> NUM_SHIFT;

/// When representing "numbers", the state has to be shifted this much (to get
/// rid of the flag bit).
const NUM_SHIFT: usize = 2;

// ===== impl Semaphore =====

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    ///
    /// # Panics
    ///
    /// Panics if `permits` is zero.
    pub(crate) fn new(permits: usize) -> Semaphore {
        let stub = Box::new(Waiter::new());
        let ptr = NonNull::from(&*stub);

        // Allocations are aligned
        debug_assert!(ptr.as_ptr() as usize & NUM_FLAG == 0);

        let state = SemState::new(permits, &stub);

        Semaphore {
            state: AtomicUsize::new(state.to_usize()),
            head: UnsafeCell::new(ptr),
            rx_lock: AtomicUsize::new(0),
            stub,
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        let curr = SemState(self.state.load(Acquire));
        curr.available_permits()
    }

    /// Tries to acquire the requested number of permits, registering the waiter
    /// if not enough permits are available.
    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        num_permits: u16,
        permit: &mut Permit,
    ) -> Poll<Result<(), AcquireError>> {
        self.poll_acquire2(num_permits, || {
            let waiter = permit.waiter.get_or_insert_with(|| Box::new(Waiter::new()));

            waiter.waker.register_by_ref(cx.waker());

            Some(NonNull::from(&**waiter))
        })
    }

    fn try_acquire(&self, num_permits: u16) -> Result<(), TryAcquireError> {
        match self.poll_acquire2(num_permits, || None) {
            Poll::Ready(res) => res.map_err(to_try_acquire),
            Poll::Pending => Err(TryAcquireError::NoPermits),
        }
    }

    /// Polls for a permit
    ///
    /// Tries to acquire available permits first. If unable to acquire a
    /// sufficient number of permits, the caller's waiter is pushed onto the
    /// semaphore's wait queue.
    fn poll_acquire2<F>(
        &self,
        num_permits: u16,
        mut get_waiter: F,
    ) -> Poll<Result<(), AcquireError>>
    where
        F: FnMut() -> Option<NonNull<Waiter>>,
    {
        let num_permits = num_permits as usize;

        // Load the current state
        let mut curr = SemState(self.state.load(Acquire));

        // Saves a ref to the waiter node
        let mut maybe_waiter: Option<NonNull<Waiter>> = None;

        /// Used in branches where we attempt to push the waiter into the wait
        /// queue but fail due to permits becoming available or the wait queue
        /// transitioning to "closed". In this case, the waiter must be
        /// transitioned back to the "idle" state.
        macro_rules! revert_to_idle {
            () => {
                if let Some(waiter) = maybe_waiter {
                    unsafe { waiter.as_ref() }.revert_to_idle();
                }
            };
        }

        loop {
            let mut next = curr;

            if curr.is_closed() {
                revert_to_idle!();
                return Ready(Err(AcquireError::closed()));
            }

            let acquired = next.acquire_permits(num_permits, &self.stub);

            if !acquired {
                // There are not enough available permits to satisfy the
                // request. The permit transitions to a waiting state.
                debug_assert!(curr.waiter().is_some() || curr.available_permits() < num_permits);

                if let Some(waiter) = maybe_waiter.as_ref() {
                    // Safety: the caller owns the waiter.
                    let w = unsafe { waiter.as_ref() };
                    w.set_permits_to_acquire(num_permits - curr.available_permits());
                } else {
                    // Get the waiter for the permit.
                    if let Some(waiter) = get_waiter() {
                        // Safety: the caller owns the waiter.
                        let w = unsafe { waiter.as_ref() };

                        // If there are any currently available permits, the
                        // waiter acquires those immediately and waits for the
                        // remaining permits to become available.
                        if !w.to_queued(num_permits - curr.available_permits()) {
                            // The node is alrady queued, there is no further work
                            // to do.
                            return Pending;
                        }

                        maybe_waiter = Some(waiter);
                    } else {
                        // No waiter, this indicates the caller does not wish to
                        // "wait", so there is nothing left to do.
                        return Pending;
                    }
                }

                next.set_waiter(maybe_waiter.unwrap());
            }

            debug_assert_ne!(curr.0, 0);
            debug_assert_ne!(next.0, 0);

            match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => {
                    if acquired {
                        // Successfully acquire permits **without** queuing the
                        // waiter node. The waiter node is not currently in the
                        // queue.
                        revert_to_idle!();
                        return Ready(Ok(()));
                    } else {
                        // The node is pushed into the queue, the final step is
                        // to set the node's "next" pointer to return the wait
                        // queue into a consistent state.

                        let prev_waiter =
                            curr.waiter().unwrap_or_else(|| NonNull::from(&*self.stub));

                        let waiter = maybe_waiter.unwrap();

                        // Link the nodes.
                        //
                        // Safety: the mpsc algorithm guarantees the old tail of
                        // the queue is not removed from the queue during the
                        // push process.
                        unsafe {
                            prev_waiter.as_ref().store_next(waiter);
                        }

                        return Pending;
                    }
                }
                Err(actual) => {
                    curr = SemState(actual);
                }
            }
        }
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        // Acquire the `rx_lock`, setting the "closed" flag on the lock.
        let prev = self.rx_lock.fetch_or(1, AcqRel);

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
        let prev = self.rx_lock.fetch_add(n << 1, AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        self.add_permits_locked(n, false);
    }

    fn add_permits_locked(&self, mut rem: usize, mut closed: bool) {
        while rem > 0 || closed {
            if closed {
                SemState::fetch_set_closed(&self.state, AcqRel);
            }

            // Release the permits and notify
            self.add_permits_locked2(rem, closed);

            let n = rem << 1;

            let actual = if closed {
                let actual = self.rx_lock.fetch_sub(n | 1, AcqRel);
                closed = false;
                actual
            } else {
                let actual = self.rx_lock.fetch_sub(n, AcqRel);
                closed = actual & 1 == 1;
                actual
            };

            rem = (actual >> 1) - rem;
        }
    }

    /// Releases a specific amount of permits to the semaphore
    ///
    /// This function is called by `add_permits` after the add lock has been
    /// acquired.
    fn add_permits_locked2(&self, mut n: usize, closed: bool) {
        // If closing the semaphore, we want to drain the entire queue. The
        // number of permits being assigned doesn't matter.
        if closed {
            n = usize::MAX;
        }

        'outer: while n > 0 {
            unsafe {
                let mut head = self.head.with(|head| *head);
                let mut next_ptr = head.as_ref().next.load(Acquire);

                let stub = self.stub();

                if head == stub {
                    // The stub node indicates an empty queue. Any remaining
                    // permits get assigned back to the semaphore.
                    let next = match NonNull::new(next_ptr) {
                        Some(next) => next,
                        None => {
                            // This loop is not part of the standard intrusive mpsc
                            // channel algorithm. This is where we atomically pop
                            // the last task and add `n` to the remaining capacity.
                            //
                            // This modification to the pop algorithm works because,
                            // at this point, we have not done any work (only done
                            // reading). We have a *pretty* good idea that there is
                            // no concurrent pusher.
                            //
                            // The capacity is then atomically added by doing an
                            // AcqRel CAS on `state`. The `state` cell is the
                            // linchpin of the algorithm.
                            //
                            // By successfully CASing `head` w/ AcqRel, we ensure
                            // that, if any thread was racing and entered a push, we
                            // see that and abort pop, retrying as it is
                            // "inconsistent".
                            let mut curr = SemState::load(&self.state, Acquire);

                            loop {
                                if curr.has_waiter(&self.stub) {
                                    // A waiter is being added concurrently.
                                    // This is the MPSC queue's "inconsistent"
                                    // state and we must loop and try again.
                                    thread::yield_now();
                                    continue 'outer;
                                }

                                // If closing, nothing more to do.
                                if closed {
                                    debug_assert!(curr.is_closed(), "state = {:?}", curr);
                                    return;
                                }

                                let mut next = curr;
                                next.release_permits(n, &self.stub);

                                match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                                    Ok(_) => return,
                                    Err(actual) => {
                                        curr = SemState(actual);
                                    }
                                }
                            }
                        }
                    };

                    self.head.with_mut(|head| *head = next);
                    head = next;
                    next_ptr = next.as_ref().next.load(Acquire);
                }

                // `head` points to a waiter assign permits to the waiter. If
                // all requested permits are satisfied, then we can continue,
                // otherwise the node stays in the wait queue.
                if !head.as_ref().assign_permits(&mut n, closed) {
                    assert_eq!(n, 0);
                    return;
                }

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    self.remove_queued(head, closed);
                    continue 'outer;
                }

                let state = SemState::load(&self.state, Acquire);

                // This must always be a pointer as the wait list is not empty.
                let tail = state.waiter().unwrap();

                if tail != head {
                    // Inconsistent
                    thread::yield_now();
                    continue 'outer;
                }

                self.push_stub(closed);

                next_ptr = head.as_ref().next.load(Acquire);

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    self.remove_queued(head, closed);
                    continue 'outer;
                }

                // Inconsistent state, loop
                thread::yield_now();
            }
        }
    }

    /// The wait node has had all of its permits assigned and has been removed
    /// from the wait queue.
    ///
    /// Attempt to remove the QUEUED bit from the node. If additional permits
    /// are concurrently requested, the node must be pushed back into the wait
    /// queued.
    fn remove_queued(&self, waiter: NonNull<Waiter>, closed: bool) {
        let mut curr = WaiterState(unsafe { waiter.as_ref() }.state.load(Acquire));

        loop {
            if curr.is_dropped() {
                // The Permit dropped, it is on us to release the memory
                let _ = unsafe { Box::from_raw(waiter.as_ptr()) };
                return;
            }

            // The node is removed from the queue. We attempt to unset the
            // queued bit, but concurrently the waiter has requested more
            // permits. When the waiter requested more permits, it saw the
            // queued bit set so took no further action. This requires us to
            // push the node back into the queue.
            if curr.permits_to_acquire() > 0 {
                // More permits are requested. The waiter must be re-queued
                unsafe {
                    self.push_waiter(waiter, closed);
                }
                return;
            }

            let mut next = curr;
            next.unset_queued();

            let w = unsafe { waiter.as_ref() };

            match w.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => return,
                Err(actual) => {
                    curr = WaiterState(actual);
                }
            }
        }
    }

    unsafe fn push_stub(&self, closed: bool) {
        self.push_waiter(self.stub(), closed);
    }

    unsafe fn push_waiter(&self, waiter: NonNull<Waiter>, closed: bool) {
        // Set the next pointer. This does not require an atomic operation as
        // this node is not accessible. The write will be flushed with the next
        // operation
        waiter.as_ref().next.store(ptr::null_mut(), Relaxed);

        // Update the tail to point to the new node. We need to see the previous
        // node in order to update the next pointer as well as release `task`
        // to any other threads calling `push`.
        let next = SemState::new_ptr(waiter, closed);
        let prev = SemState(self.state.swap(next.0, AcqRel));

        debug_assert_eq!(closed, prev.is_closed());

        // This function is only called when there are pending tasks. Because of
        // this, the state must *always* be in pointer mode.
        let prev = prev.waiter().unwrap();

        // No cycles plz
        debug_assert_ne!(prev, waiter);

        // Release `task` to the consume end.
        prev.as_ref().next.store(waiter.as_ptr(), Release);
    }

    fn stub(&self) -> NonNull<Waiter> {
        unsafe { NonNull::new_unchecked(&*self.stub as *const _ as *mut _) }
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
            .field("state", &SemState::load(&self.state, Relaxed))
            .field("head", &self.head.with(|ptr| ptr))
            .field("rx_lock", &self.rx_lock.load(Relaxed))
            .field("stub", &self.stub)
            .finish()
    }
}

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

// ===== impl Permit =====

impl Permit {
    /// Creates a new `Permit`.
    ///
    /// The permit begins in the "unacquired" state.
    pub(crate) fn new() -> Permit {
        use PermitState::Acquired;

        Permit {
            waiter: None,
            state: Acquired(0),
        }
    }

    /// Returns `true` if the permit has been acquired
    #[allow(dead_code)] // may be used later
    pub(crate) fn is_acquired(&self) -> bool {
        match self.state {
            PermitState::Acquired(num) if num > 0 => true,
            _ => false,
        }
    }

    /// Tries to acquire the permit. If no permits are available, the current task
    /// is notified once a new permit becomes available.
    pub(crate) fn poll_acquire(
        &mut self,
        cx: &mut Context<'_>,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Poll<Result<(), AcquireError>> {
        use std::cmp::Ordering::*;
        use PermitState::*;

        match self.state {
            Waiting(requested) => {
                // There must be a waiter
                let waiter = self.waiter.as_ref().unwrap();

                match requested.cmp(&num_permits) {
                    Less => {
                        let delta = num_permits - requested;

                        // Request additional permits. If the waiter has been
                        // dequeued, it must be re-queued.
                        if !waiter.try_inc_permits_to_acquire(delta as usize) {
                            let waiter = NonNull::from(&**waiter);

                            // Ignore the result. The check for
                            // `permits_to_acquire()` will converge the state as
                            // needed
                            let _ = semaphore.poll_acquire2(delta, || Some(waiter))?;
                        }

                        self.state = Waiting(num_permits);
                    }
                    Greater => {
                        let delta = requested - num_permits;
                        let to_release = waiter.try_dec_permits_to_acquire(delta as usize);

                        semaphore.add_permits(to_release);
                        self.state = Waiting(num_permits);
                    }
                    Equal => {}
                }

                if waiter.permits_to_acquire()? == 0 {
                    self.state = Acquired(requested);
                    return Ready(Ok(()));
                }

                waiter.waker.register_by_ref(cx.waker());

                if waiter.permits_to_acquire()? == 0 {
                    self.state = Acquired(requested);
                    return Ready(Ok(()));
                }

                Pending
            }
            Acquired(acquired) => {
                if acquired >= num_permits {
                    Ready(Ok(()))
                } else {
                    match semaphore.poll_acquire(cx, num_permits - acquired, self)? {
                        Ready(()) => {
                            self.state = Acquired(num_permits);
                            Ready(Ok(()))
                        }
                        Pending => {
                            self.state = Waiting(num_permits);
                            Pending
                        }
                    }
                }
            }
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
            Waiting(requested) => {
                // There must be a waiter
                let waiter = self.waiter.as_ref().unwrap();

                if requested > num_permits {
                    let delta = requested - num_permits;
                    let to_release = waiter.try_dec_permits_to_acquire(delta as usize);

                    semaphore.add_permits(to_release);
                    self.state = Waiting(num_permits);
                }

                let res = waiter.permits_to_acquire().map_err(to_try_acquire)?;

                if res == 0 {
                    if requested < num_permits {
                        // Try to acquire the additional permits
                        semaphore.try_acquire(num_permits - requested)?;
                    }

                    self.state = Acquired(num_permits);
                    Ok(())
                } else {
                    Err(TryAcquireError::NoPermits)
                }
            }
            Acquired(acquired) => {
                if acquired < num_permits {
                    semaphore.try_acquire(num_permits - acquired)?;
                    self.state = Acquired(num_permits);
                }

                Ok(())
            }
        }
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
            Waiting(requested) => {
                let n = cmp::min(n, requested);

                // Decrement
                let acquired = self
                    .waiter
                    .as_ref()
                    .unwrap()
                    .try_dec_permits_to_acquire(n as usize) as u16;

                if n == requested {
                    self.state = Acquired(0);
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

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            // Set the dropped flag
            let state = WaiterState(waiter.state.fetch_or(DROPPED, AcqRel));

            if state.is_queued() {
                // The waiter is stored in the queue. The semaphore will drop it
                std::mem::forget(waiter);
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

// ===== impl Waiter =====

impl Waiter {
    fn new() -> Waiter {
        Waiter {
            state: AtomicUsize::new(0),
            waker: AtomicWaker::new(),
            next: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn permits_to_acquire(&self) -> Result<usize, AcquireError> {
        let state = WaiterState(self.state.load(Acquire));

        if state.is_closed() {
            Err(AcquireError(()))
        } else {
            Ok(state.permits_to_acquire())
        }
    }

    /// Only increments the number of permits *if* the waiter is currently
    /// queued.
    ///
    /// # Returns
    ///
    /// `true` if the number of permits to acquire has been incremented. `false`
    /// otherwise. On `false`, the caller should use `Semaphore::poll_acquire`.
    fn try_inc_permits_to_acquire(&self, n: usize) -> bool {
        let mut curr = WaiterState(self.state.load(Acquire));

        loop {
            if !curr.is_queued() {
                assert_eq!(0, curr.permits_to_acquire());
                return false;
            }

            let mut next = curr;
            next.set_permits_to_acquire(n + curr.permits_to_acquire());

            match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => return true,
                Err(actual) => curr = WaiterState(actual),
            }
        }
    }

    /// Try to decrement the number of permits to acquire. This returns the
    /// actual number of permits that were decremented. The delta betweeen `n`
    /// and the return has been assigned to the permit and the caller must
    /// assign these back to the semaphore.
    fn try_dec_permits_to_acquire(&self, n: usize) -> usize {
        let mut curr = WaiterState(self.state.load(Acquire));

        loop {
            if !curr.is_queued() {
                assert_eq!(0, curr.permits_to_acquire());
            }

            let delta = cmp::min(n, curr.permits_to_acquire());
            let rem = curr.permits_to_acquire() - delta;

            let mut next = curr;
            next.set_permits_to_acquire(rem);

            match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => return n - delta,
                Err(actual) => curr = WaiterState(actual),
            }
        }
    }

    /// Store the number of remaining permits needed to satisfy the waiter and
    /// transition to the "QUEUED" state.
    ///
    /// # Returns
    ///
    /// `true` if the `QUEUED` bit was set as part of the transition.
    fn to_queued(&self, num_permits: usize) -> bool {
        let mut curr = WaiterState(self.state.load(Acquire));

        // The waiter should **not** be waiting for any permits.
        debug_assert_eq!(curr.permits_to_acquire(), 0);

        loop {
            let mut next = curr;
            next.set_permits_to_acquire(num_permits);
            next.set_queued();

            match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => {
                    if curr.is_queued() {
                        return false;
                    } else {
                        // Make sure the next pointer is null
                        self.next.store(ptr::null_mut(), Relaxed);
                        return true;
                    }
                }
                Err(actual) => curr = WaiterState(actual),
            }
        }
    }

    /// Set the number of permits to acquire.
    ///
    /// This function is only called when the waiter is being inserted into the
    /// wait queue. Because of this, there are no concurrent threads that can
    /// modify the state and using `store` is safe.
    fn set_permits_to_acquire(&self, num_permits: usize) {
        debug_assert!(WaiterState(self.state.load(Acquire)).is_queued());

        let mut state = WaiterState(QUEUED);
        state.set_permits_to_acquire(num_permits);

        self.state.store(state.0, Release);
    }

    /// Assign permits to the waiter.
    ///
    /// Returns `true` if the waiter should be removed from the queue
    fn assign_permits(&self, n: &mut usize, closed: bool) -> bool {
        let mut curr = WaiterState(self.state.load(Acquire));

        loop {
            let mut next = curr;

            // Number of permits to assign to this waiter
            let assign = cmp::min(curr.permits_to_acquire(), *n);

            // Assign the permits
            next.set_permits_to_acquire(curr.permits_to_acquire() - assign);

            if closed {
                next.set_closed();
            }

            match self.state.compare_exchange(curr.0, next.0, AcqRel, Acquire) {
                Ok(_) => {
                    // Update `n`
                    *n -= assign;

                    if next.permits_to_acquire() == 0 {
                        if curr.permits_to_acquire() > 0 {
                            self.waker.wake();
                        }

                        return true;
                    } else {
                        return false;
                    }
                }
                Err(actual) => curr = WaiterState(actual),
            }
        }
    }

    fn revert_to_idle(&self) {
        // An idle node is not waiting on any permits
        self.state.store(0, Relaxed);
    }

    fn store_next(&self, next: NonNull<Waiter>) {
        self.next.store(next.as_ptr(), Release);
    }
}

// ===== impl SemState =====

impl SemState {
    /// Returns a new default `State` value.
    fn new(permits: usize, stub: &Waiter) -> SemState {
        assert!(permits <= MAX_PERMITS);

        if permits > 0 {
            SemState((permits << NUM_SHIFT) | NUM_FLAG)
        } else {
            SemState(stub as *const _ as usize)
        }
    }

    /// Returns a `State` tracking `ptr` as the tail of the queue.
    fn new_ptr(tail: NonNull<Waiter>, closed: bool) -> SemState {
        let mut val = tail.as_ptr() as usize;

        if closed {
            val |= CLOSED_FLAG;
        }

        SemState(val)
    }

    /// Returns the amount of remaining capacity
    fn available_permits(self) -> usize {
        if !self.has_available_permits() {
            return 0;
        }

        self.0 >> NUM_SHIFT
    }

    /// Returns `true` if the state has permits that can be claimed by a waiter.
    fn has_available_permits(self) -> bool {
        self.0 & NUM_FLAG == NUM_FLAG
    }

    fn has_waiter(self, stub: &Waiter) -> bool {
        !self.has_available_permits() && !self.is_stub(stub)
    }

    /// Tries to atomically acquire specified number of permits.
    ///
    /// # Return
    ///
    /// Returns `true` if the specified number of permits were acquired, `false`
    /// otherwise. Returning false does not mean that there are no more
    /// available permits.
    fn acquire_permits(&mut self, num: usize, stub: &Waiter) -> bool {
        debug_assert!(num > 0);

        if self.available_permits() < num {
            return false;
        }

        debug_assert!(self.waiter().is_none());

        self.0 -= num << NUM_SHIFT;

        if self.0 == NUM_FLAG {
            // Set the state to the stub pointer.
            self.0 = stub as *const _ as usize;
        }

        true
    }

    /// Releases permits
    ///
    /// Returns `true` if the permits were accepted.
    fn release_permits(&mut self, permits: usize, stub: &Waiter) {
        debug_assert!(permits > 0);

        if self.is_stub(stub) {
            self.0 = (permits << NUM_SHIFT) | NUM_FLAG | (self.0 & CLOSED_FLAG);
            return;
        }

        debug_assert!(self.has_available_permits());

        self.0 += permits << NUM_SHIFT;
    }

    fn is_waiter(self) -> bool {
        self.0 & NUM_FLAG == 0
    }

    /// Returns the waiter, if one is set.
    fn waiter(self) -> Option<NonNull<Waiter>> {
        if self.is_waiter() {
            let waiter = NonNull::new(self.as_ptr()).expect("null pointer stored");

            Some(waiter)
        } else {
            None
        }
    }

    /// Assumes `self` represents a pointer
    fn as_ptr(self) -> *mut Waiter {
        (self.0 & !CLOSED_FLAG) as *mut Waiter
    }

    /// Sets to a pointer to a waiter.
    ///
    /// This can only be done from the full state.
    fn set_waiter(&mut self, waiter: NonNull<Waiter>) {
        let waiter = waiter.as_ptr() as usize;
        debug_assert!(!self.is_closed());

        self.0 = waiter;
    }

    fn is_stub(self, stub: &Waiter) -> bool {
        self.as_ptr() as usize == stub as *const _ as usize
    }

    /// Loads the state from an AtomicUsize.
    fn load(cell: &AtomicUsize, ordering: Ordering) -> SemState {
        let value = cell.load(ordering);
        SemState(value)
    }

    fn fetch_set_closed(cell: &AtomicUsize, ordering: Ordering) -> SemState {
        let value = cell.fetch_or(CLOSED_FLAG, ordering);
        SemState(value)
    }

    fn is_closed(self) -> bool {
        self.0 & CLOSED_FLAG == CLOSED_FLAG
    }

    /// Converts the state into a `usize` representation.
    fn to_usize(self) -> usize {
        self.0
    }
}

impl fmt::Debug for SemState {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = fmt.debug_struct("SemState");

        if self.is_waiter() {
            fmt.field("state", &"<waiter>");
        } else {
            fmt.field("permits", &self.available_permits());
        }

        fmt.finish()
    }
}

// ===== impl WaiterState =====

impl WaiterState {
    fn permits_to_acquire(self) -> usize {
        self.0 >> PERMIT_SHIFT
    }

    fn set_permits_to_acquire(&mut self, val: usize) {
        self.0 = (val << PERMIT_SHIFT) | (self.0 & !PERMIT_MASK)
    }

    fn is_queued(self) -> bool {
        self.0 & QUEUED == QUEUED
    }

    fn set_queued(&mut self) {
        self.0 |= QUEUED;
    }

    fn is_closed(self) -> bool {
        self.0 & CLOSED == CLOSED
    }

    fn set_closed(&mut self) {
        self.0 |= CLOSED;
    }

    fn unset_queued(&mut self) {
        assert!(self.is_queued());
        self.0 -= QUEUED;
    }

    fn is_dropped(self) -> bool {
        self.0 & DROPPED == DROPPED
    }
}
