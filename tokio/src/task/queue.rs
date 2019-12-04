use super::{OwnedList, Schedule, Task, TransferStack};
use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    fmt,
    sync::{Mutex, MutexGuard},
};

/// A set of multi-producer, single consumer task queues, suitable for use by a
/// single-threaded scheduler.
///
/// This consists of a list of _all_ tasks bound to the scheduler, a run queue
/// of tasks notified from the thread the scheduler is running on (the "local
/// queue"), a run queue of tasks notified from another thread (the "remote
/// queue"), and a stack of tasks released from other threads which will
/// eventually need to be dropped by the scheduler on its own thread ("pending
/// drop").
///
/// Submitting tasks to or popping tasks from the local queue is unsafe, as it
/// must only be performed on the same thread as the scheduler.
pub(crate) struct MpscQueues<S: 'static> {
    /// List of all active tasks spawned onto this executor.
    ///
    /// # Safety
    ///
    /// Must only be accessed from the primary thread
    owned_tasks: UnsafeCell<OwnedList<S>>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    ///
    /// # Safety
    ///
    /// References should not be handed out. Only call `push` / `pop` functions.
    /// Only call from the owning thread.
    local_queue: UnsafeCell<VecDeque<Task<S>>>,

    /// Remote run queue.
    ///
    /// Tasks notified from another thread are pushed into this queue.
    remote_queue: Mutex<RemoteQueue<S>>,

    /// Tasks pending drop
    pending_drop: TransferStack<S>,
}

pub(crate) struct RemoteQueue<S: 'static> {
    /// FIFO list of tasks
    queue: VecDeque<Task<S>>,

    /// `true` when a task can be pushed into the queue, false otherwise.
    open: bool,
}

// === impl Queues ===

impl<S> MpscQueues<S>
where
    S: Schedule + 'static,
{
    pub(crate) const INITIAL_CAPACITY: usize = 64;

    /// How often to check the remote queue first
    pub(crate) const CHECK_REMOTE_INTERVAL: u8 = 13;

    pub(crate) fn new() -> Self {
        Self {
            owned_tasks: UnsafeCell::new(OwnedList::new()),
            local_queue: UnsafeCell::new(VecDeque::with_capacity(Self::INITIAL_CAPACITY)),
            pending_drop: TransferStack::new(),
            remote_queue: Mutex::new(RemoteQueue {
                queue: VecDeque::with_capacity(Self::INITIAL_CAPACITY),
                open: true,
            }),
        }
    }

    /// Add a new task to the scheduler.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn add_task(&self, task: &Task<S>) {
        (*self.owned_tasks.get()).insert(task);
    }

    /// Push a task to the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn push_local(&self, task: Task<S>) {
        (*self.local_queue.get()).push_back(task);
    }

    /// Remove a task from the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn release_local(&self, task: &Task<S>) {
        (*self.owned_tasks.get()).remove(task);
    }

    /// Lock the remote queue, returning a `MutexGuard`.
    ///
    /// This can be used to push to the remote queue and perform other
    /// operations while holding the lock.
    ///
    /// # Panics
    ///
    /// If the remote queue mutex is poisoned.
    pub(crate) fn remote(&self) -> MutexGuard<'_, RemoteQueue<S>> {
        self.remote_queue
            .lock()
            .expect("failed to lock remote queue")
    }

    /// Release a task from outside of the thread that owns the scheduler.
    ///
    /// This simply pushes the task to the pending drop queue.
    pub(crate) fn release_remote(&self, task: Task<S>) {
        self.pending_drop.push(task);
    }

    /// Returns the next task from the remote *or* local queue.
    ///
    /// Typically, this checks the local queue before the remote queue, and only
    /// checks the remote queue if the local queue is empty. However, to avoid
    /// starving the remote queue, it is checked first every
    /// `CHECK_REMOTE_INTERVAL` ticks.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn next_task(&self, tick: u8) -> Option<Task<S>> {
        if 0 == tick % Self::CHECK_REMOTE_INTERVAL {
            self.next_remote_task().or_else(|| self.next_local_task())
        } else {
            self.next_local_task().or_else(|| self.next_remote_task())
        }
    }

    /// Returns the next task from the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn next_local_task(&self) -> Option<Task<S>> {
        (*self.local_queue.get()).pop_front()
    }

    /// Returns the next task from the remote queue.
    ///
    /// # Panics
    ///
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    pub(crate) fn next_remote_task(&self) -> Option<Task<S>> {
        // there is no semantic information in the `PoisonError`, and it
        // doesn't implement `Debug`, but clippy thinks that it's bad to
        // match all errors here...
        #[allow(clippy::match_wild_err_arm)]
        let mut lock = match self.remote_queue.lock() {
            // If the lock is poisoned, but the thread is already panicking,
            // avoid a double panic. This is necessary since `next_task` (which
            // calls `next_remote_task`) can be called in the `Drop` impl.
            Err(_) if std::thread::panicking() => return None,
            Err(_) => panic!("mutex poisoned"),
            Ok(lock) => lock,
        };
        lock.queue.pop_front()
    }

    /// Returns true if any owned tasks are still bound to this scheduler.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn has_tasks_remaining(&self) -> bool {
        !(*self.owned_tasks.get()).is_empty()
    }

    /// Drain any tasks that have previously been released from other threads.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn drain_pending_drop(&self) {
        for task in self.pending_drop.drain() {
            (*self.owned_tasks.get()).remove(&task);
            drop(task);
        }
    }

    /// Shut down the queues.
    ///
    /// This performs the following operations:
    ///
    /// 1. Close the remote queue (so that it will no longer accept new tasks).
    /// 2. Drain the remote queue and shut down all tasks.
    /// 3. Drain the local queue and shut down all tasks.
    /// 4. Shut down the owned task list.
    /// 5. Drain the list of tasks dropped externally and remove them from the
    ///    owned task list.
    ///
    /// This method should be called before dropping a `Queues`. It is provided
    /// as a method rather than a `Drop` impl because types that own a `Queues`
    /// wish to perform other work in their `Drop` implementations _after_
    /// shutting down the task queues.
    ///
    /// # Safety
    ///
    /// This method accesses the local task queue, and therefore *must* be
    /// called only from the thread that owns the scheduler.
    ///
    /// # Panics
    ///
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    pub(crate) unsafe fn shutdown(&self) {
        // Close and drain the remote queue.
        self.close_remote();

        // Drain the local queue.
        self.close_local();

        // Release owned tasks
        self.shutdown_owned_tasks();

        // Drain tasks pending drop.
        self.drain_pending_drop();
    }

    /// Shut down the scheduler's owned task list.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    unsafe fn shutdown_owned_tasks(&self) {
        (*self.owned_tasks.get()).shutdown();
    }

    /// Drain the remote queue, and shut down its tasks.
    ///
    /// This closes the remote queue. Any additional tasks added to it will be
    /// shut down instead.
    ///
    /// # Panics
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    fn close_remote(&self) {
        #[allow(clippy::match_wild_err_arm)]
        let mut lock = match self.remote_queue.lock() {
            // If the lock is poisoned, but the thread is already panicking,
            // avoid a double panic. This is necessary since this fn can be
            // called in a drop impl.
            Err(_) if std::thread::panicking() => return,
            Err(_) => panic!("mutex poisoned"),
            Ok(lock) => lock,
        };
        lock.open = false;

        while let Some(task) = lock.queue.pop_front() {
            task.shutdown();
        }
    }

    /// Drain the local queue, and shut down its tasks.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    unsafe fn close_local(&self) {
        while let Some(task) = self.next_local_task() {
            task.shutdown();
        }
    }
}

impl<S> fmt::Debug for MpscQueues<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("MpscQueues")
            .field("owned_tasks", &self.owned_tasks)
            .field("remote_queue", &self.remote_queue)
            .field("local_queue", &self.local_queue)
            .finish()
    }
}

// === impl RemoteQueue ===

impl<S> RemoteQueue<S>
where
    S: Schedule,
{
    /// Schedule a remote task.
    ///
    /// If the queue is open to accept new tasks, the task is pushed to the back
    /// of the queue. Otherwise, if the queue is closed (the scheduler is
    /// shutting down), the new task will be shut down immediately.
    pub(crate) fn schedule(&mut self, task: Task<S>) {
        if self.open {
            self.queue.push_back(task);
        } else {
            task.shutdown();
        }
    }
}

impl<S> fmt::Debug for RemoteQueue<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("RemoteQueue")
            .field("queue", &self.queue)
            .field("open", &self.open)
            .finish()
    }
}
