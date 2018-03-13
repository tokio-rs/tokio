use std::fmt;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};

use Task;

/// A synchronization primitive for task notification.
///
/// `AtomicTask` will coordinate concurrent notifications with the consumer
/// potentially "updating" the underlying task to notify. This is useful in
/// scenarios where a computation completes in another thread and wants to
/// notify the consumer, but the consumer is in the process of being migrated to
/// a new logical task.
///
/// Consumers should call `register` before checking the result of a computation
/// and producers should call `notify` after producing the computation (this
/// differs from the usual `thread::park` pattern). It is also permitted for
/// `notify` to be called **before** `register`. This results in a no-op.
///
/// A single `AtomicTask` may be reused for any number of calls to `register` or
/// `notify`.
///
/// `AtomicTask` does not provide any memory ordering guarantees, as such the
/// user should use caution and use other synchronization primitives to guard
/// the result of the underlying computation.
pub struct AtomicTask {
    state: AtomicUsize,
    task: UnsafeCell<Option<Task>>,
}

/// Initial state, the `AtomicTask` is currently not being used.
///
/// The value `2` is picked specifically because it between the write lock &
/// read lock values. Since the read lock is represented by an incrementing
/// counter, this enables an atomic fetch_sub operation to be used for releasing
/// a lock.
const WAITING: usize = 2;

/// The `register` function has determined that the task is no longer current.
/// This implies that `AtomicTask::register` is being called from a different
/// task than is represented by the currently stored task. The write lock is
/// obtained to update the task cell.
const LOCKED_WRITE: usize = 0;

/// At least one call to `notify` happened concurrently to `register` updating
/// the task cell. This state is detected when `register` exits the mutation
/// code and signals to `register` that it is responsible for notifying its own
/// task.
const LOCKED_WRITE_NOTIFIED: usize = 1;


/// The `notify` function has locked access to the task cell for notification.
///
/// The constant is left here mostly for documentation reasons.
#[allow(dead_code)]
const LOCKED_READ: usize = 3;

impl AtomicTask {
    /// Create an `AtomicTask` initialized with the given `Task`
    pub fn new() -> AtomicTask {
        // Make sure that task is Sync
        trait AssertSync: Sync {}
        impl AssertSync for Task {}

        AtomicTask {
            state: AtomicUsize::new(WAITING),
            task: UnsafeCell::new(None),
        }
    }

    /// Registers the task to be notified on calls to `notify`.
    ///
    /// The new task will take place of any previous tasks that were registered
    /// by previous calls to `register`. Any calls to `notify` that happen after
    /// a call to `register` (as defined by the memory ordering rules), will
    /// notify the `register` caller's task.
    ///
    /// It is safe to call `register` with multiple other threads concurrently
    /// calling `notify`. This will result in the `register` caller's current
    /// task being notified once.
    ///
    /// This function is safe to call concurrently, but this is generally a bad
    /// idea. Concurrent calls to `register` will attempt to register different
    /// tasks to be notified. One of the callers will win and have its task set,
    /// but there is no guarantee as to which caller will succeed.
    pub(crate) fn register(&self, task: Task) {
        match self.state.compare_and_swap(WAITING, LOCKED_WRITE, Acquire) {
            WAITING => {
                unsafe {
                    // Locked acquired, update the task cell
                    *self.task.get() = Some(task);

                    // Release the lock. If the state transitioned to
                    // `LOCKED_NOTIFIED`, this means that an notify has been
                    // signaled, so notify the task.
                    if LOCKED_WRITE_NOTIFIED == self.state.swap(WAITING, Release) {
                        (*self.task.get()).as_ref().unwrap().notify();
                    }
                }
            }
            LOCKED_WRITE | LOCKED_WRITE_NOTIFIED => {
                // A thread is concurrently calling `register`. This shouldn't
                // happen as it doesn't really make much sense, but it isn't
                // unsafe per se. Since two threads are concurrently trying to
                // update the task, it's undefined which one "wins" (no ordering
                // guarantees), so we can just do nothing.
            }
            state => {
                debug_assert!(state != LOCKED_WRITE, "unexpected state LOCKED_WRITE");
                debug_assert!(state != LOCKED_WRITE_NOTIFIED, "unexpected state LOCKED_WRITE_NOTIFIED");

                // Currently in a read locked state, this implies that `notify`
                // is currently being called on the old task handle. So, we call
                // notify on the new task handle
                task.notify();
            }
        }
    }

    /// Notifies the task that last called `register`.
    ///
    /// If `register` has not been called yet, then this does nothing.
    pub fn notify(&self) {
        let mut curr = WAITING;

        loop {
            if curr == LOCKED_WRITE {
                // Transition the state to LOCKED_NOTIFIED
                let actual = self.state.compare_and_swap(LOCKED_WRITE, LOCKED_WRITE_NOTIFIED, Release);

                if curr == actual {
                    // Success, return
                    return;
                }

                // update current state variable and try again
                curr = actual;

            } else if curr == LOCKED_WRITE_NOTIFIED {
                // Currently in `LOCKED_WRITE_NOTIFIED` state, nothing else to do.
                return;

            } else {
                // Currently in a LOCKED_READ state, so attempt to increment the
                // lock count.
                let actual = self.state.compare_and_swap(curr, curr + 1, Acquire);

                // Locked acquired
                if actual == curr {
                    // Notify the task
                    unsafe {
                        if let Some(ref task) = (*self.task.get()).take() {
                            task.notify();
                        }
                    }

                    // Release the lock
                    self.state.fetch_sub(1, Release);

                    // Done
                    return;
                }

                // update current state variable and try again
                curr = actual;

            }
        }
    }
}

impl Default for AtomicTask {
    fn default() -> Self {
        AtomicTask::new()
    }
}

impl fmt::Debug for AtomicTask {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "AtomicTask")
    }
}

unsafe impl Send for AtomicTask {}
unsafe impl Sync for AtomicTask {}
