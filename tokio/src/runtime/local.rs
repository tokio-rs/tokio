//! Groups a set of tasks that execute on the same thread.
use crate::executor::park::{Park, Unpark};
use crate::executor::task::{self, JoinHandle, Schedule, Task};

use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use pin_project::{pin_project, project};

/// A group of tasks which are executed on the same thread.
///
/// These tasks need not implement `Send`; a local task set provides the
/// capacity to execute `!Send` futures.
#[derive(Debug)]
#[pin_project]
pub struct TaskSet<F> {
    scheduler: Scheduler,
    #[pin]
    future: F,
}

struct Scheduler {
    /// List of all active tasks spawned onto this executor.
    ///
    /// # Safety
    ///
    /// Must only be accessed from the primary thread
    owned_tasks: UnsafeCell<task::OwnedList<Scheduler>>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    ///
    /// # Safety
    ///
    /// References should not be handed out. Only call `push` / `pop` functions.
    /// Only call from the owning thread.
    queue: UnsafeCell<VecDeque<Task<Scheduler>>>,
}

thread_local! {
    static CURRENT_TASK_SET: Cell<Option<NonNull<Scheduler>>> = Cell::new(None);
}

/// Returns a local task set for the given future.
///
/// The provided future may call `spawn_local`.
pub fn task_set<F>(future: F) -> TaskSet<F>
where
    F: Future + 'static,
    F::Output: 'static,
{
    TaskSet::new(f)
}

/// Spawns a `!Send` future on the local task set.
///
/// The spawned future will be run on the same thread that called `spawn_local.`
/// This may only be called from the context of a local task set.
///
/// # Panics
/// - This function panics if called outside of a local task set.
pub fn spawn_local<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    CURRENT_TASK_SET.with(|current| {
        let current = current
            .get()
            .expect("`local::spawn` called from outside of a local::TaskSet!");
        unsafe {
            let scheduler = scheduler.as_ref();
            let (task, handle) = task::joinable_unsend(future);
            set.schedule(task);
            handle
        }
    })
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

impl<F> TaskSet<F>
where
    F: Future + 'static,
    F::Output: 'static,
{
    /// Returns a new local task set for the given future.
    pub fn new(future: F) -> Self {
        Self {
            scheduler: Scheduler::new(),
            future,
        }
    }
}

impl<F: Future> Future for TaskSet<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let TaskSet { scheduler, future } = self.project();
        scheduler.with(|| {
            if let Poll::Ready(v) = future.poll(&mut cx) {
                return Poll::Ready(v);
            }

            scheduler.tick(local);

            match future.poll(&mut cx) {
                Poll::Ready(v) => Poll::Ready(v),
                Poll::Pending => {
                    cx.waker().wake();
                    Poll::Pending
                }
            }
        })
    }
}

// === impl Scheduler ===

impl Schedule for Scheduler {
    fn bind(&self, task: &Task<Self>) {
        assert!(self.is_current());
        unsafe {
            (*self.owned_tasks.get()).insert(task);
        }
    }

    fn release(&self, _: Task<Self>) {
        unreachable!("tasks should only be completed locally")
    }

    fn release_local(&self, task: &Task<Self>) {
        assert!(self.is_current());
        unsafe {
            (*self.owned_tasks.get()).remove(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        assert!(self.is_current());
        unsafe {
            (*self.queue.get()).push_front(task);
        }
    }
}

impl Scheduler {
    fn new() -> Self {
        Self {
            owned_tasks: UnsafeCell::new(task::OwnedList::new()),
            queue: UnsafeCell::new(VecDeque::with_capacity(64)),
        }
    }

    fn with<F>(&mut self, f: impl FnOnce() -> F) -> F {
        struct Entered<'a> {
            current: &'a Cell<Option<NonNull<Scheduler>>>,
        }

        impl<'a> Drop for Entered<'a> {
            fn drop(&mut self) {
                self.current.set(None);
            }
        }

        CURRENT_TASK_SET.with(|current| {
            current.set(Some(NonNull::from(self)));
            let _entered = Entered { current };
            f()
        })
    }

    fn is_current(&self) -> bool {
        CURRENT_TASK_SET
            .try_with(|current| {
                current
                    .get()
                    .iter()
                    .any(|current| ptr::eq(current.as_ptr(), self as *const _))
            })
            .unwrap_or(false)
    }

    fn next_task(&self) -> Option<Task<Self>> {
        unsafe { (*self.task.get()).pop_front() }
    }

    fn tick(&self) {
        for _ in 0..MAX_TASKS_PER_TICK {
            let task = match self.next_task() {
                Some(task) => task,
                None => return,
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                unsafe {
                    self.schedule_local(task);
                }
            }
        }
    }
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Scheduler { .. }").finish()
    }
}
