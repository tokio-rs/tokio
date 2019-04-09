//! Thread-safe, asynchronous counting semaphore.
//!
//! A `Semaphore` instance holds a set of permits. Permits are used to
//! synchronize access to a shared resource.
//!
//! Before accessing the shared resource, callers acquire a permit from the
//! semaphore. Once the permit is acquired, the caller then enters the critical
//! section. If no permits are available, then acquiring the semaphore returns
//! `NotReady`. The task is notified once a permit becomes available.

use loom::{
    futures::AtomicTask,
    sync::{
        atomic::{AtomicPtr, AtomicUsize},
        CausalCell,
    },
    yield_now,
};

use futures::Poll;

use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{self, AcqRel, Acquire, Relaxed, Release};
use std::sync::Arc;
use std::usize;

/// Futures-aware semaphore.
pub struct Semaphore {
    /// Tracks both the waiter queue tail pointer and the number of remaining
    /// permits.
    state: AtomicUsize,

    /// waiter queue head pointer.
    head: CausalCell<NonNull<WaiterNode>>,

    /// Coordinates access to the queue head.
    rx_lock: AtomicUsize,

    /// Stub waiter node used as part of the MPSC channel algorithm.
    stub: Box<WaiterNode>,
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
pub struct Permit {
    waiter: Option<Arc<WaiterNode>>,
    state: PermitState,
}

/// Error returned by `Permit::poll_acquire`.
#[derive(Debug)]
pub struct AcquireError(());

/// Error returned by `Permit::try_acquire`.
#[derive(Debug)]
pub struct TryAcquireError {
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    Closed,
    NoPermits,
}

/// Node used to notify the semaphore waiter when permit is available.
#[derive(Debug)]
struct WaiterNode {
    /// Stores waiter state.
    ///
    /// See `NodeState` for more details.
    state: AtomicUsize,

    /// Task to notify when a permit is made available.
    task: AtomicTask,

    /// Next pointer in the queue of waiting senders.
    next: AtomicPtr<WaiterNode>,
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
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PermitState {
    /// The permit has not been requested.
    Idle,

    /// Currently waiting for a permit to be made available and assigned to the
    /// waiter.
    Waiting,

    /// The permit has been acquired.
    Acquired,
}

/// Waiter node state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(usize)]
enum NodeState {
    /// Not waiting for a permit and the node is not in the wait queue.
    ///
    /// This is the initial state.
    Idle = 0,

    /// Not waiting for a permit but the node is in the wait queue.
    ///
    /// This happens when the waiter has previously requested a permit, but has
    /// since canceled the request. The node cannot be removed by the waiter, so
    /// this state informs the receiver to skip the node when it pops it from
    /// the wait queue.
    Queued = 1,

    /// Waiting for a permit and the node is in the wait queue.
    QueuedWaiting = 2,

    /// The waiter has been assigned a permit and the node has been removed from
    /// the queue.
    Assigned = 3,

    /// The semaphore has been closed. No more permits will be issued.
    Closed = 4,
}

// ===== impl Semaphore =====

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    ///
    /// # Panics
    ///
    /// Panics if `permits` is zero.
    pub fn new(permits: usize) -> Semaphore {
        let stub = Box::new(WaiterNode::new());
        let ptr = NonNull::new(&*stub as *const _ as *mut _).unwrap();

        // Allocations are aligned
        debug_assert!(ptr.as_ptr() as usize & NUM_FLAG == 0);

        let state = SemState::new(permits, &stub);

        Semaphore {
            state: AtomicUsize::new(state.to_usize()),
            head: CausalCell::new(ptr),
            rx_lock: AtomicUsize::new(0),
            stub,
        }
    }

    /// Returns the current number of available permits
    pub fn available_permits(&self) -> usize {
        let curr = SemState::load(&self.state, Acquire);
        curr.available_permits()
    }

    /// Poll for a permit
    fn poll_permit(&self, mut permit: Option<&mut Permit>) -> Poll<(), AcquireError> {
        use futures::Async::*;

        // Load the current state
        let mut curr = SemState::load(&self.state, Acquire);

        debug!(" + poll_permit; sem-state = {:?}", curr);

        // Tracks a *mut WaiterNode representing an Arc clone.
        //
        // This avoids having to bump the ref count unless required.
        let mut maybe_strong: Option<NonNull<WaiterNode>> = None;

        macro_rules! undo_strong {
            () => {
                if let Some(waiter) = maybe_strong {
                    // The waiter was cloned, but never got queued.
                    // Before entering `poll_permit`, the waiter was in the
                    // `Idle` state. We must transition the node back to the
                    // idle state.
                    let waiter = unsafe { Arc::from_raw(waiter.as_ptr()) };
                    waiter.revert_to_idle();
                }
            };
        }

        loop {
            let mut next = curr;

            if curr.is_closed() {
                undo_strong!();
                return Err(AcquireError::closed());
            }

            if !next.acquire_permit(&self.stub) {
                debug!(" + poll_permit -- no permits");

                debug_assert!(curr.waiter().is_some());

                if maybe_strong.is_none() {
                    if let Some(ref mut permit) = permit {
                        // Get the Sender's waiter node, or initialize one
                        let waiter = permit
                            .waiter
                            .get_or_insert_with(|| Arc::new(WaiterNode::new()));

                        waiter.register();

                        debug!(" + poll_permit -- to_queued_waiting");

                        if !waiter.to_queued_waiting() {
                            debug!(" + poll_permit; waiter already queued");
                            // The node is alrady queued, there is no further work
                            // to do.
                            return Ok(NotReady);
                        }

                        maybe_strong = Some(WaiterNode::into_non_null(waiter.clone()));
                    } else {
                        // If no `waiter`, then the task is not registered and there
                        // is no further work to do.
                        return Ok(NotReady);
                    }
                }

                next.set_waiter(maybe_strong.unwrap());
            }

            debug!(" + poll_permit -- pre-CAS; next = {:?}", next);

            debug_assert_ne!(curr.0, 0);
            debug_assert_ne!(next.0, 0);

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => {
                    debug!(" + poll_permit -- CAS ok");
                    match curr.waiter() {
                        Some(prev_waiter) => {
                            let waiter = maybe_strong.unwrap();

                            // Finish pushing
                            unsafe {
                                prev_waiter.as_ref().next.store(waiter.as_ptr(), Release);
                            }

                            debug!(" + poll_permit -- waiter pushed");

                            return Ok(NotReady);
                        }
                        None => {
                            debug!(" + poll_permit -- permit acquired");

                            undo_strong!();

                            return Ok(Ready(()));
                        }
                    }
                }
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    /// Close the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub fn close(&self) {
        debug!("+ Semaphore::close");

        // Acquire the `rx_lock`, setting the "closed" flag on the lock.
        let prev = self.rx_lock.fetch_or(1, AcqRel);
        debug!(" + close -- rx_lock.fetch_add(1)");

        if prev != 0 {
            debug!("+ close -- locked; prev = {}", prev);
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        self.add_permits_locked(0, true);
    }

    /// Add `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        debug!(" + add_permits; n = {}", n);

        if n == 0 {
            return;
        }

        // TODO: Handle overflow. A panic is not sufficient, the process must
        // abort.
        let prev = self.rx_lock.fetch_add(n << 1, AcqRel);
        debug!(" + add_permits; rx_lock.fetch_add(n << 1); n = {}", n);

        if prev != 0 {
            debug!(" + add_permits -- locked; prev = {}", prev);
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        self.add_permits_locked(n, false);
    }

    fn add_permits_locked(&self, mut rem: usize, mut closed: bool) {
        while rem > 0 || closed {
            debug!(
                " + add_permits_locked -- iter; rem = {}; closed = {:?}",
                rem, closed
            );

            if closed {
                SemState::fetch_set_closed(&self.state, AcqRel);
            }

            // Release the permits and notify
            self.add_permits_locked2(rem, closed);

            let n = rem << 1;

            let actual = if closed {
                let actual = self.rx_lock.fetch_sub(n | 1, AcqRel);
                debug!(
                    " + add_permits_locked; rx_lock.fetch_sub(n | 1); n = {}; actual={}",
                    n, actual
                );

                closed = false;
                actual
            } else {
                let actual = self.rx_lock.fetch_sub(n, AcqRel);
                debug!(
                    " + add_permits_locked; rx_lock.fetch_sub(n); n = {}; actual={}",
                    n, actual
                );

                closed = actual & 1 == 1;
                actual
            };

            rem = (actual >> 1) - rem;
        }

        debug!(" + add_permits; done");
    }

    /// Release a specific amount of permits to the semaphore
    ///
    /// This function is called by `add_permits` after the add lock has been
    /// acquired.
    fn add_permits_locked2(&self, mut n: usize, closed: bool) {
        while n > 0 || closed {
            let waiter = match self.pop(n, closed) {
                Some(waiter) => waiter,
                None => {
                    return;
                }
            };

            debug!(" + release_n -- notify");

            if waiter.notify(closed) {
                n = n.saturating_sub(1);
                debug!(" + release_n -- dec");
            }
        }
    }

    /// Pop a waiter
    ///
    /// `rem` represents the remaining number of times the caller will pop. If
    /// there are no more waiters to pop, `rem` is used to set the available
    /// permits.
    fn pop(&self, rem: usize, closed: bool) -> Option<Arc<WaiterNode>> {
        debug!(" + pop; rem = {}", rem);

        'outer: loop {
            unsafe {
                let mut head = self.head.with(|head| *head);
                let mut next_ptr = head.as_ref().next.load(Acquire);

                let stub = self.stub();

                if head == stub {
                    debug!(" + pop; head == stub");

                    let next = match NonNull::new(next_ptr) {
                        Some(next) => next,
                        None => {
                            // This loop is not part of the standard intrusive mpsc
                            // channel algorithm. This is where we atomically pop
                            // the last task and add `rem` to the remaining capacity.
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
                                    // Inconsistent
                                    debug!(" + pop; inconsistent 1");
                                    yield_now();
                                    continue 'outer;
                                }

                                // When closing the semaphore, nodes are popped
                                // with `rem == 0`. In this case, we are not
                                // adding permits, but notifying waiters of the
                                // semaphore's closed state.
                                if rem == 0 {
                                    debug_assert!(curr.is_closed(), "state = {:?}", curr);
                                    return None;
                                }

                                let mut next = curr;
                                next.release_permits(rem, &self.stub);

                                match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                                    Ok(_) => return None,
                                    Err(actual) => {
                                        curr = actual;
                                    }
                                }
                            }
                        }
                    };

                    debug!(" + pop; got next waiter");

                    self.head.with_mut(|head| *head = next);
                    head = next;
                    next_ptr = next.as_ref().next.load(Acquire);
                }

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    return Some(Arc::from_raw(head.as_ptr()));
                }

                let state = SemState::load(&self.state, Acquire);

                // This must always be a pointer as the wait list is not empty.
                let tail = state.waiter().unwrap();

                if tail != head {
                    // Inconsistent
                    debug!(" + pop; inconsistent 2");
                    yield_now();
                    continue 'outer;
                }

                self.push_stub(closed);

                next_ptr = head.as_ref().next.load(Acquire);

                if let Some(next) = NonNull::new(next_ptr) {
                    self.head.with_mut(|head| *head = next);

                    return Some(Arc::from_raw(head.as_ptr()));
                }

                // Inconsistent state, loop
                debug!(" + pop; inconsistent 3");
                yield_now();
            }
        }
    }

    unsafe fn push_stub(&self, closed: bool) {
        let stub = self.stub();

        // Set the next pointer. This does not require an atomic operation as
        // this node is not accessible. The write will be flushed with the next
        // operation
        stub.as_ref().next.store(ptr::null_mut(), Relaxed);

        // Update the tail to point to the new node. We need to see the previous
        // node in order to update the next pointer as well as release `task`
        // to any other threads calling `push`.
        let prev = SemState::new_ptr(stub, closed).swap(&self.state, AcqRel);

        debug_assert_eq!(closed, prev.is_closed());

        // The stub is only pushed when there are pending tasks. Because of
        // this, the state must *always* be in pointer mode.
        let prev = prev.waiter().unwrap();

        // We don't want the *existing* pointer to be a stub.
        debug_assert_ne!(prev, stub);

        // Release `task` to the consume end.
        prev.as_ref().next.store(stub.as_ptr(), Release);
    }

    fn stub(&self) -> NonNull<WaiterNode> {
        unsafe { NonNull::new_unchecked(&*self.stub as *const _ as *mut _) }
    }
}

impl fmt::Debug for Semaphore {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
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
    /// Create a new `Permit`.
    ///
    /// The permit begins in the "unacquired" state.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_sync::semaphore::Permit;
    ///
    /// let permit = Permit::new();
    /// assert!(!permit.is_acquired());
    /// ```
    pub fn new() -> Permit {
        Permit {
            waiter: None,
            state: PermitState::Idle,
        }
    }

    /// Returns true if the permit has been acquired
    pub fn is_acquired(&self) -> bool {
        self.state == PermitState::Acquired
    }

    /// Try to acquire the permit. If no permits are available, the current task
    /// is notified once a new permit becomes available.
    pub fn poll_acquire(&mut self, semaphore: &Semaphore) -> Poll<(), AcquireError> {
        use futures::Async::*;

        match self.state {
            PermitState::Idle => {}
            PermitState::Waiting => {
                let waiter = self.waiter.as_ref().unwrap();

                if waiter.acquire()? {
                    self.state = PermitState::Acquired;
                    return Ok(Ready(()));
                } else {
                    return Ok(NotReady);
                }
            }
            PermitState::Acquired => {
                return Ok(Ready(()));
            }
        }

        match semaphore.poll_permit(Some(self))? {
            Ready(v) => {
                self.state = PermitState::Acquired;
                Ok(Ready(v))
            }
            NotReady => {
                self.state = PermitState::Waiting;
                Ok(NotReady)
            }
        }
    }

    /// Try to acquire the permit.
    pub fn try_acquire(&mut self, semaphore: &Semaphore) -> Result<(), TryAcquireError> {
        use futures::Async::*;

        match self.state {
            PermitState::Idle => {}
            PermitState::Waiting => {
                let waiter = self.waiter.as_ref().unwrap();

                if waiter.acquire2().map_err(to_try_acquire)? {
                    self.state = PermitState::Acquired;
                    return Ok(());
                } else {
                    return Err(TryAcquireError::no_permits());
                }
            }
            PermitState::Acquired => {
                return Ok(());
            }
        }

        match semaphore.poll_permit(None).map_err(to_try_acquire)? {
            Ready(()) => {
                self.state = PermitState::Acquired;
                Ok(())
            }
            NotReady => Err(TryAcquireError::no_permits()),
        }
    }

    /// Release a permit back to the semaphore
    pub fn release(&mut self, semaphore: &Semaphore) {
        if self.forget2() {
            semaphore.add_permits(1);
        }
    }

    /// Forget the permit **without** releasing it back to the semaphore.
    ///
    /// After calling `forget`, `poll_acquire` is able to acquire new permit
    /// from the sempahore.
    ///
    /// Repeatedly calling `forget` without associated calls to `add_permit`
    /// will result in the semaphore losing all permits.
    pub fn forget(&mut self) {
        self.forget2();
    }

    /// Returns `true` if the permit was acquired
    fn forget2(&mut self) -> bool {
        match self.state {
            PermitState::Idle => false,
            PermitState::Waiting => {
                let ret = self.waiter.as_ref().unwrap().cancel_interest();
                self.state = PermitState::Idle;
                ret
            }
            PermitState::Acquired => {
                self.state = PermitState::Idle;
                true
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
    TryAcquireError::closed()
}

impl fmt::Display for AcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for AcquireError {
    fn description(&self) -> &str {
        "semaphore closed"
    }
}

// ===== impl TryAcquireError =====

impl TryAcquireError {
    fn closed() -> TryAcquireError {
        TryAcquireError {
            kind: ErrorKind::Closed,
        }
    }

    fn no_permits() -> TryAcquireError {
        TryAcquireError {
            kind: ErrorKind::NoPermits,
        }
    }

    /// Returns true if the error was caused by a closed semaphore.
    pub fn is_closed(&self) -> bool {
        match self.kind {
            ErrorKind::Closed => true,
            _ => false,
        }
    }

    /// Returns true if the error was caused by calling `try_acquire` on a
    /// semaphore with no available permits.
    pub fn is_no_permits(&self) -> bool {
        match self.kind {
            ErrorKind::NoPermits => true,
            _ => false,
        }
    }
}

impl fmt::Display for TryAcquireError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for TryAcquireError {
    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::Closed => "semaphore closed",
            ErrorKind::NoPermits => "no permits available",
        }
    }
}

// ===== impl WaiterNode =====

impl WaiterNode {
    fn new() -> WaiterNode {
        WaiterNode {
            state: AtomicUsize::new(NodeState::new().to_usize()),
            task: AtomicTask::new(),
            next: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn acquire(&self) -> Result<bool, AcquireError> {
        if self.acquire2()? {
            return Ok(true);
        }

        self.task.register();

        self.acquire2()
    }

    fn acquire2(&self) -> Result<bool, AcquireError> {
        use self::NodeState::*;

        match Idle.compare_exchange(&self.state, Assigned, AcqRel, Acquire) {
            Ok(_) => Ok(true),
            Err(Closed) => Err(AcquireError::closed()),
            Err(_) => Ok(false),
        }
    }

    fn register(&self) {
        self.task.register()
    }

    /// Returns `true` if the permit has been acquired
    fn cancel_interest(&self) -> bool {
        use self::NodeState::*;

        match Queued.compare_exchange(&self.state, QueuedWaiting, AcqRel, Acquire) {
            // Successfully removed interest from the queued node. The permit
            // has not been assigned to the node.
            Ok(_) => false,
            // The semaphore has been closed, there is no further action to
            // take.
            Err(Closed) => false,
            // The permit has been assigned. It must be acquired in order to
            // be released back to the semaphore.
            Err(Assigned) => {
                match self.acquire2() {
                    Ok(true) => true,
                    // Not a reachable state
                    Ok(false) => panic!(),
                    // The semaphore has been closed, no further action to take.
                    Err(_) => false,
                }
            }
            Err(state) => panic!("unexpected state = {:?}", state),
        }
    }

    /// Transition the state to `QueuedWaiting`.
    ///
    /// This step can only happen from `Queued` or from `Idle`.
    ///
    /// Returns `true` if transitioning into a queued state.
    fn to_queued_waiting(&self) -> bool {
        use self::NodeState::*;

        let mut curr = NodeState::load(&self.state, Acquire);

        loop {
            debug_assert!(curr == Idle || curr == Queued, "actual = {:?}", curr);
            let next = QueuedWaiting;

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => {
                    if curr.is_queued() {
                        return false;
                    } else {
                        // Transitioned to queued, reset next pointer
                        self.next.store(ptr::null_mut(), Relaxed);
                        return true;
                    }
                }
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    /// Notify the waiter
    ///
    /// Returns `true` if the waiter accepts the notification
    fn notify(&self, closed: bool) -> bool {
        use self::NodeState::*;

        // Assume QueuedWaiting state
        let mut curr = QueuedWaiting;

        loop {
            let next = match curr {
                Queued => Idle,
                QueuedWaiting => {
                    if closed {
                        Closed
                    } else {
                        Assigned
                    }
                }
                actual => panic!("actual = {:?}", actual),
            };

            match next.compare_exchange(&self.state, curr, AcqRel, Acquire) {
                Ok(_) => match curr {
                    QueuedWaiting => {
                        debug!(" + notify -- task notified");
                        self.task.notify();
                        return true;
                    }
                    other => {
                        debug!(" + notify -- not notified; state = {:?}", other);
                        return false;
                    }
                },
                Err(actual) => curr = actual,
            }
        }
    }

    fn revert_to_idle(&self) {
        use self::NodeState::Idle;

        // There are no other handles to the node
        NodeState::store(&self.state, Idle, Relaxed);
    }

    fn into_non_null(arc: Arc<WaiterNode>) -> NonNull<WaiterNode> {
        let ptr = Arc::into_raw(arc);
        unsafe { NonNull::new_unchecked(ptr as *mut _) }
    }
}

// ===== impl State =====

/// Flag differentiating between available permits and waiter pointers.
///
/// If we assume pointers are properly aligned, then the least significant bit
/// will always be zero. So, we use that bit to track if the value represents a
/// number.
const NUM_FLAG: usize = 0b01;

const CLOSED_FLAG: usize = 0b10;

const MAX_PERMITS: usize = usize::MAX >> NUM_SHIFT;

/// When representing "numbers", the state has to be shifted this much (to get
/// rid of the flag bit).
const NUM_SHIFT: usize = 2;

impl SemState {
    /// Returns a new default `State` value.
    fn new(permits: usize, stub: &WaiterNode) -> SemState {
        assert!(permits <= MAX_PERMITS);

        if permits > 0 {
            SemState((permits << NUM_SHIFT) | NUM_FLAG)
        } else {
            SemState(stub as *const _ as usize)
        }
    }

    /// Returns a `State` tracking `ptr` as the tail of the queue.
    fn new_ptr(tail: NonNull<WaiterNode>, closed: bool) -> SemState {
        let mut val = tail.as_ptr() as usize;

        if closed {
            val |= CLOSED_FLAG;
        }

        SemState(val)
    }

    /// Returns the amount of remaining capacity
    fn available_permits(&self) -> usize {
        if !self.has_available_permits() {
            return 0;
        }

        self.0 >> NUM_SHIFT
    }

    /// Returns true if the state has permits that can be claimed by a waiter.
    fn has_available_permits(&self) -> bool {
        self.0 & NUM_FLAG == NUM_FLAG
    }

    fn has_waiter(&self, stub: &WaiterNode) -> bool {
        !self.has_available_permits() && !self.is_stub(stub)
    }

    /// Try to acquire a permit
    ///
    /// # Return
    ///
    /// Returns `true` if the permit was acquired, `false` otherwise. If `false`
    /// is returned, it can be assumed that `State` represents the head pointer
    /// in the mpsc channel.
    fn acquire_permit(&mut self, stub: &WaiterNode) -> bool {
        if !self.has_available_permits() {
            return false;
        }

        debug_assert!(self.waiter().is_none());

        self.0 -= 1 << NUM_SHIFT;

        if self.0 == NUM_FLAG {
            // Set the state to the stub pointer.
            self.0 = stub as *const _ as usize;
        }

        true
    }

    /// Release permits
    ///
    /// Returns `true` if the permits were accepted.
    fn release_permits(&mut self, permits: usize, stub: &WaiterNode) {
        debug_assert!(permits > 0);

        if self.is_stub(stub) {
            self.0 = (permits << NUM_SHIFT) | NUM_FLAG | (self.0 & CLOSED_FLAG);
            return;
        }

        debug_assert!(self.has_available_permits());

        self.0 += permits << NUM_SHIFT;
    }

    fn is_waiter(&self) -> bool {
        self.0 & NUM_FLAG == 0
    }

    /// Returns the waiter, if one is set.
    fn waiter(&self) -> Option<NonNull<WaiterNode>> {
        if self.is_waiter() {
            let waiter = NonNull::new(self.as_ptr()).expect("null pointer stored");

            Some(waiter)
        } else {
            None
        }
    }

    /// Assumes `self` represents a pointer
    fn as_ptr(&self) -> *mut WaiterNode {
        (self.0 & !CLOSED_FLAG) as *mut WaiterNode
    }

    /// Set to a pointer to a waiter.
    ///
    /// This can only be done from the full state.
    fn set_waiter(&mut self, waiter: NonNull<WaiterNode>) {
        let waiter = waiter.as_ptr() as usize;
        debug_assert!(waiter & NUM_FLAG == 0);
        debug_assert!(!self.is_closed());

        self.0 = waiter;
    }

    fn is_stub(&self, stub: &WaiterNode) -> bool {
        self.as_ptr() as usize == stub as *const _ as usize
    }

    /// Load the state from an AtomicUsize.
    fn load(cell: &AtomicUsize, ordering: Ordering) -> SemState {
        let value = cell.load(ordering);
        debug!(" + SemState::load; value = {}", value);
        SemState(value)
    }

    /// Swap the values
    fn swap(&self, cell: &AtomicUsize, ordering: Ordering) -> SemState {
        let prev = SemState(cell.swap(self.to_usize(), ordering));
        debug_assert_eq!(prev.is_closed(), self.is_closed());
        prev
    }

    /// Compare and exchange the current value into the provided cell
    fn compare_exchange(
        &self,
        cell: &AtomicUsize,
        prev: SemState,
        success: Ordering,
        failure: Ordering,
    ) -> Result<SemState, SemState> {
        debug_assert_eq!(prev.is_closed(), self.is_closed());

        let res = cell.compare_exchange(prev.to_usize(), self.to_usize(), success, failure);

        debug!(
            " + SemState::compare_exchange; prev = {}; next = {}; result = {:?}",
            prev.to_usize(),
            self.to_usize(),
            res
        );

        res.map(SemState).map_err(SemState)
    }

    fn fetch_set_closed(cell: &AtomicUsize, ordering: Ordering) -> SemState {
        let value = cell.fetch_or(CLOSED_FLAG, ordering);
        SemState(value)
    }

    fn is_closed(&self) -> bool {
        self.0 & CLOSED_FLAG == CLOSED_FLAG
    }

    /// Converts the state into a `usize` representation.
    fn to_usize(&self) -> usize {
        self.0
    }
}

impl fmt::Debug for SemState {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut fmt = fmt.debug_struct("SemState");

        if self.is_waiter() {
            fmt.field("state", &"<waiter>");
        } else {
            fmt.field("permits", &self.available_permits());
        }

        fmt.finish()
    }
}

// ===== impl NodeState =====

impl NodeState {
    fn new() -> NodeState {
        NodeState::Idle
    }

    fn from_usize(value: usize) -> NodeState {
        use self::NodeState::*;

        match value {
            0 => Idle,
            1 => Queued,
            2 => QueuedWaiting,
            3 => Assigned,
            4 => Closed,
            _ => panic!(),
        }
    }

    fn load(cell: &AtomicUsize, ordering: Ordering) -> NodeState {
        NodeState::from_usize(cell.load(ordering))
    }

    /// Store a value
    fn store(cell: &AtomicUsize, value: NodeState, ordering: Ordering) {
        cell.store(value.to_usize(), ordering);
    }

    fn compare_exchange(
        &self,
        cell: &AtomicUsize,
        prev: NodeState,
        success: Ordering,
        failure: Ordering,
    ) -> Result<NodeState, NodeState> {
        cell.compare_exchange(prev.to_usize(), self.to_usize(), success, failure)
            .map(NodeState::from_usize)
            .map_err(NodeState::from_usize)
    }

    /// Returns `true` if `self` represents a queued state.
    fn is_queued(&self) -> bool {
        use self::NodeState::*;

        match *self {
            Queued | QueuedWaiting => true,
            _ => false,
        }
    }

    fn to_usize(&self) -> usize {
        *self as usize
    }
}
