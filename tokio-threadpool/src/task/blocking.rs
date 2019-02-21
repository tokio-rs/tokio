use pool::Pool;
use task::{BlockingState, Task};

use futures::{Async, Poll};

use std::cell::UnsafeCell;
use std::fmt;
use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::Arc;
use std::thread;

/// Manages the state around entering a blocking section and tasks that are
/// queued pending the ability to block.
///
/// This is a hybrid counter and intrusive mpsc channel (like `Queue`).
#[derive(Debug)]
pub(crate) struct Blocking {
    /// Queue head.
    ///
    /// This is either the current remaining capacity for blocking sections
    /// **or** if the max has been reached, the head of a pending blocking
    /// capacity channel of tasks.
    ///
    /// When this points to a task, it represents a strong reference, i.e.
    /// `Arc<Task>`.
    state: AtomicUsize,

    /// Tail pointer. This is `Arc<Task>` unless it points to `stub`.
    tail: UnsafeCell<*mut Task>,

    /// Stub pointer, used as part of the intrusive mpsc channel algorithm
    /// described by 1024cores.
    stub: Box<Task>,

    /// The channel algorithm is MPSC. This means that, in order to pop tasks,
    /// coordination is required.
    ///
    /// Since it doesn't matter *which* task pops & notifies the queued task, we
    /// can avoid a full mutex and make the "lock" lock free.
    ///
    /// Instead, threads race to set the "entered" bit. When the transition is
    /// successfully made, the thread has permission to pop tasks off of the
    /// queue. If a thread loses the race, instead of waiting to pop a task, it
    /// signals to the winning thread that it should pop an additional task.
    lock: AtomicUsize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum CanBlock {
    /// Blocking capacity has been allocated to this task.
    ///
    /// The capacity allocation is initially checked before a task is polled. If
    /// capacity has been allocated, it is consumed and tracked as `Allocated`.
    Allocated,

    /// Allocation capacity must be either available to the task when it is
    /// polled or not available. This means that a task can only ask for
    /// capacity once. This state is used to track a task that has not yet asked
    /// for blocking capacity. When a task needs blocking capacity, if it is in
    /// this state, it can immediately try to get an allocation.
    CanRequest,

    /// The task has requested blocking capacity, but none is available.
    NoCapacity,
}

/// Decorates the `usize` value of `Blocking::state`, providing fns to
/// manipulate the state instead of requiring bit ops.
#[derive(Copy, Clone, Eq, PartialEq)]
struct State(usize);

/// Flag differentiating between remaining capacity and task pointers.
///
/// If we assume pointers are properly aligned, then the least significant bit
/// will always be zero. So, we use that bit to track if the value represents a
/// number.
const NUM_FLAG: usize = 1;

/// When representing "numbers", the state has to be shifted this much (to get
/// rid of the flag bit).
const NUM_SHIFT: usize = 1;

// ====== impl Blocking =====
//
impl Blocking {
    /// Create a new `Blocking`.
    pub fn new(capacity: usize) -> Blocking {
        assert!(capacity > 0, "blocking capacity must be greater than zero");

        let stub = Box::new(Task::stub());
        let ptr = &*stub as *const _ as *mut _;

        // Allocations are aligned
        debug_assert!(ptr as usize & NUM_FLAG == 0);

        // The initial state value. This starts at the max capacity.
        let init = State::new(capacity);

        Blocking {
            state: AtomicUsize::new(init.into()),
            tail: UnsafeCell::new(ptr),
            stub: stub,
            lock: AtomicUsize::new(0),
        }
    }

    /// Atomically either acquire blocking capacity or queue the task to be
    /// notified once capacity becomes available.
    ///
    /// The caller must ensure that `task` has not previously been queued to be
    /// notified when capacity becomes available.
    pub fn poll_blocking_capacity(&self, task: &Arc<Task>) -> Poll<(), ::BlockingError> {
        // This requires atomically claiming blocking capacity and if none is
        // available, queuing &task.

        // The task cannot be queued at this point. The caller must ensure this.
        debug_assert!(!BlockingState::from(task.blocking.load(Acquire)).is_queued());

        // Don't bump the ref count unless necessary.
        let mut strong: Option<*const Task> = None;

        // Load the state
        let mut curr: State = self.state.load(Acquire).into();

        loop {
            let mut next = curr;

            if !next.claim_capacity(&self.stub) {
                debug_assert!(curr.ptr().is_some());

                // Unable to claim capacity, so we must queue `task` onto the
                // channel.
                //
                // This guard also serves to ensure that queuing work that is
                // only needed to run once only gets run once.
                if strong.is_none() {
                    // First, transition the task to a "queued" state. This
                    // prevents double queuing.
                    //
                    // This is also the only thread that can set the queued flag
                    // at this point. And, the goal is for this to only be
                    // visible when the task node is polled from the channel.
                    // The memory ordering is established by MPSC queue
                    // operation.
                    //
                    // Note that, if the task doesn't get queued (because the
                    // CAS fails and capacity is now available) then this flag
                    // must be unset. Again, there is no race because until the
                    // task is queued, no other thread can see it.
                    let prev = BlockingState::toggle_queued(&task.blocking, Relaxed);
                    debug_assert!(!prev.is_queued());

                    // Bump the ref count
                    strong = Some(Arc::into_raw(task.clone()));

                    // Set the next pointer. This does not require an atomic
                    // operation as this node is not currently accessible to
                    // other threads via the queue.
                    task.next_blocking.store(ptr::null_mut(), Relaxed);
                }

                let ptr = strong.unwrap();

                // Update the head to point to the new node. We need to see the
                // previous node in order to update the next pointer as well as
                // release `task` to any other threads calling `push`.
                next.set_ptr(ptr);
            }

            debug_assert_ne!(curr.0, 0);
            debug_assert_ne!(next.0, 0);

            let actual = self
                .state
                .compare_and_swap(curr.into(), next.into(), AcqRel)
                .into();

            if curr == actual {
                break;
            }

            curr = actual;
        }

        match curr.ptr() {
            Some(prev) => {
                let ptr = strong.unwrap();

                // Finish pushing
                unsafe {
                    (*prev).next_blocking.store(ptr as *mut _, Release);
                }

                // The node was queued to be notified once capacity is made
                // available.
                Ok(Async::NotReady)
            }
            None => {
                debug_assert!(curr.remaining_capacity() > 0);

                // If `strong` is set, gotta undo a bunch of work
                if let Some(ptr) = strong {
                    let _ = unsafe { Arc::from_raw(ptr) };

                    // Unset the queued flag.
                    let prev = BlockingState::toggle_queued(&task.blocking, Relaxed);
                    debug_assert!(prev.is_queued());
                }

                // Capacity has been obtained
                Ok(().into())
            }
        }
    }

    unsafe fn push_stub(&self) {
        let task: *mut Task = &*self.stub as *const _ as *mut _;

        // Set the next pointer. This does not require an atomic operation as
        // this node is not accessible. The write will be flushed with the next
        // operation
        (*task).next_blocking.store(ptr::null_mut(), Relaxed);

        // Update the head to point to the new node. We need to see the previous
        // node in order to update the next pointer as well as release `task`
        // to any other threads calling `push`.
        let prev = self.state.swap(task as usize, AcqRel);

        // The stub is only pushed when there are pending tasks. Because of
        // this, the state must *always* be in pointer mode.
        debug_assert!(State::from(prev).is_ptr());

        let prev = prev as *const Task;

        // We don't want the *existing* pointer to be a stub.
        debug_assert_ne!(prev, task);

        // Release `task` to the consume end.
        (*prev).next_blocking.store(task, Release);
    }

    pub fn notify_task(&self, pool: &Arc<Pool>) {
        let prev = self.lock.fetch_add(1, AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending tasks.
            return;
        }

        let mut dec = 1;

        loop {
            let mut remaining_pops = dec;
            while remaining_pops > 0 {
                remaining_pops -= 1;

                let task = match self.pop(remaining_pops) {
                    Some(t) => t,
                    None => break,
                };

                Task::notify_blocking(task, pool);
            }

            // Decrement the number of handled notifications
            let actual = self.lock.fetch_sub(dec, AcqRel);

            if actual == dec {
                break;
            }

            // This can only be greater than expected as we are the only thread
            // that is decrementing.
            debug_assert!(actual > dec);
            dec = actual - dec;
        }
    }

    /// Pop a task
    ///
    /// `rem` represents the remaining number of times the caller will pop. If
    /// there are no more tasks to pop, `rem` is used to set the remaining
    /// capacity.
    fn pop(&self, rem: usize) -> Option<Arc<Task>> {
        'outer: loop {
            unsafe {
                let mut tail = *self.tail.get();
                let mut next = (*tail).next_blocking.load(Acquire);

                let stub = &*self.stub as *const _ as *mut _;

                if tail == stub {
                    if next.is_null() {
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
                        let mut curr: State = self.state.load(Acquire).into();

                        loop {
                            if curr.has_task(&self.stub) {
                                // Inconsistent state, yield the thread and try
                                // again.
                                thread::yield_now();
                                continue 'outer;
                            }

                            let mut after = curr;

                            // +1 here because `rem` represents the number of
                            // pops that will come after the current one.
                            after.add_capacity(rem + 1, &self.stub);

                            let actual: State = self
                                .state
                                .compare_and_swap(curr.into(), after.into(), AcqRel)
                                .into();

                            if actual == curr {
                                // Successfully returned the remaining capacity
                                return None;
                            }

                            curr = actual;
                        }
                    }

                    *self.tail.get() = next;
                    tail = next;
                    next = (*next).next_blocking.load(Acquire);
                }

                if !next.is_null() {
                    *self.tail.get() = next;

                    // No ref_count inc is necessary here as this poll is paired
                    // with a `push` which "forgets" the handle.
                    return Some(Arc::from_raw(tail));
                }

                let state = self.state.load(Acquire);

                // This must always be a pointer
                debug_assert!(State::from(state).is_ptr());

                if state != tail as usize {
                    // Try again
                    thread::yield_now();
                    continue 'outer;
                }

                self.push_stub();

                next = (*tail).next_blocking.load(Acquire);

                if !next.is_null() {
                    *self.tail.get() = next;

                    return Some(Arc::from_raw(tail));
                }

                thread::yield_now();
                // Try again
            }
        }
    }
}

// ====== impl State =====

impl State {
    /// Return a new `State` representing the remaining capacity at the maximum
    /// value.
    fn new(capacity: usize) -> State {
        State((capacity << NUM_SHIFT) | NUM_FLAG)
    }

    fn remaining_capacity(&self) -> usize {
        if !self.has_remaining_capacity() {
            return 0;
        }

        self.0 >> 1
    }

    fn has_remaining_capacity(&self) -> bool {
        self.0 & NUM_FLAG == NUM_FLAG
    }

    fn has_task(&self, stub: &Task) -> bool {
        !(self.has_remaining_capacity() || self.is_stub(stub))
    }

    fn is_stub(&self, stub: &Task) -> bool {
        self.0 == stub as *const _ as usize
    }

    /// Try to claim blocking capacity.
    ///
    /// # Return
    ///
    /// Returns `true` if the capacity was claimed, `false` otherwise. If
    /// `false` is returned, it can be assumed that `State` represents the head
    /// pointer in the mpsc channel.
    fn claim_capacity(&mut self, stub: &Task) -> bool {
        if !self.has_remaining_capacity() {
            return false;
        }

        debug_assert!(self.0 != 1);

        self.0 -= 1 << NUM_SHIFT;

        if self.0 == NUM_FLAG {
            // Set the state to the stub pointer.
            self.0 = stub as *const _ as usize;
        }

        true
    }

    /// Add blocking capacity.
    fn add_capacity(&mut self, capacity: usize, stub: &Task) -> bool {
        debug_assert!(capacity > 0);

        if self.is_stub(stub) {
            self.0 = (capacity << NUM_SHIFT) | NUM_FLAG;
            true
        } else if self.has_remaining_capacity() {
            self.0 += capacity << NUM_SHIFT;
            true
        } else {
            false
        }
    }

    fn is_ptr(&self) -> bool {
        self.0 & NUM_FLAG == 0
    }

    fn ptr(&self) -> Option<*const Task> {
        if self.is_ptr() {
            Some(self.0 as *const Task)
        } else {
            None
        }
    }

    fn set_ptr(&mut self, ptr: *const Task) {
        let ptr = ptr as usize;
        debug_assert!(ptr & NUM_FLAG == 0);
        self.0 = ptr
    }
}

impl From<usize> for State {
    fn from(src: usize) -> State {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> usize {
        src.0
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut fmt = fmt.debug_struct("State");

        if self.is_ptr() {
            fmt.field("ptr", &self.0);
        } else {
            fmt.field("remaining", &self.remaining_capacity());
        }

        fmt.finish()
    }
}
