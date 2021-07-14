//! Core task module.
//!
//! # Safety
//!
//! The functions in this module are private to the `task` module. All of them
//! should be considered `unsafe` to use, but are not marked as such since it
//! would be too noisy.
//!
//! Make sure to consult the relevant safety section of each function before
//! use.

use crate::future::Future;
use crate::loom::cell::UnsafeCell;
use crate::runtime::task::raw::{self, Vtable};
use crate::runtime::task::state::State;
use crate::runtime::task::{Notified, Schedule, Task};
use crate::util::linked_list;

use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{Context, Poll, Waker};

/// The task cell. Contains the components of the task.
///
/// It is critical for `Header` to be the first field as the task structure will
/// be referenced by both *mut Cell and *mut Header.
#[repr(C)]
pub(super) struct Cell<T: Future, S> {
    /// Hot task state data
    pub(super) header: Header,

    /// Either the future or output, depending on the execution stage.
    pub(super) core: Core<T, S>,

    /// Cold data
    pub(super) trailer: Trailer,
}

pub(super) struct Scheduler<S> {
    scheduler: UnsafeCell<Option<S>>,
}

pub(super) struct CoreStage<T: Future> {
    stage: UnsafeCell<Stage<T>>,
}

/// The core of the task.
///
/// Holds the future or output, depending on the stage of execution.
pub(super) struct Core<T: Future, S> {
    /// Scheduler used to drive this future
    pub(super) scheduler: Scheduler<S>,

    /// Either the future or the output
    pub(super) stage: CoreStage<T>,
}

/// Crate public as this is also needed by the pool.
#[repr(C)]
pub(crate) struct Header {
    /// Task state
    pub(super) state: State,

    pub(crate) owned: UnsafeCell<linked_list::Pointers<Header>>,

    /// Pointer to next task, used with the injection queue
    pub(crate) queue_next: UnsafeCell<Option<NonNull<Header>>>,

    /// Table of function pointers for executing actions on the task.
    pub(super) vtable: &'static Vtable,

    /// The tracing ID for this instrumented task.
    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(super) id: Option<tracing::Id>,
}

unsafe impl Send for Header {}
unsafe impl Sync for Header {}

/// Cold data is stored after the future.
pub(super) struct Trailer {
    /// Consumer task waiting on completion of this task.
    pub(super) waker: UnsafeCell<Option<Waker>>,
}

/// Either the future or the output.
pub(super) enum Stage<T: Future> {
    Running(T),
    Finished(super::Result<T::Output>),
    Consumed,
}

impl<T: Future, S: Schedule> Cell<T, S> {
    /// Allocates a new task cell, containing the header, trailer, and core
    /// structures.
    pub(super) fn new(future: T, scheduler: S, state: State) -> Box<Cell<T, S>> {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let id = future.id();
        Box::new(Cell {
            header: Header {
                state,
                owned: UnsafeCell::new(linked_list::Pointers::new()),
                queue_next: UnsafeCell::new(None),
                vtable: raw::vtable::<T, S>(),
                #[cfg(all(tokio_unstable, feature = "tracing"))]
                id,
            },
            core: Core {
                scheduler: Scheduler {
                    scheduler: UnsafeCell::new(Some(scheduler)),
                },
                stage: CoreStage {
                    stage: UnsafeCell::new(Stage::Running(future)),
                },
            },
            trailer: Trailer {
                waker: UnsafeCell::new(None),
            },
        })
    }
}

impl<S: Schedule> Scheduler<S> {
    pub(super) fn with_mut<R>(&self, f: impl FnOnce(*mut Option<S>) -> R) -> R {
        self.scheduler.with_mut(f)
    }

    /// Returns true if the task is bound to a scheduler.
    pub(super) fn is_bound(&self) -> bool {
        // Safety: never called concurrently w/ a mutation.
        self.scheduler.with(|ptr| unsafe { (*ptr).is_some() })
    }

    /// Schedule the future for execution
    pub(super) fn schedule(&self, task: Notified<S>) {
        self.scheduler.with(|ptr| {
            // Safety: Can only be called after initial `poll`, which is the
            // only time the field is mutated.
            match unsafe { &*ptr } {
                Some(scheduler) => scheduler.schedule(task),
                None => panic!("no scheduler set"),
            }
        });
    }

    /// Schedule the future for execution in the near future, yielding the
    /// thread to other tasks.
    pub(super) fn yield_now(&self, task: Notified<S>) {
        self.scheduler.with(|ptr| {
            // Safety: Can only be called after initial `poll`, which is the
            // only time the field is mutated.
            match unsafe { &*ptr } {
                Some(scheduler) => scheduler.yield_now(task),
                None => panic!("no scheduler set"),
            }
        });
    }

    /// Release the task
    ///
    /// If the `Scheduler` implementation is able to, it returns the `Task`
    /// handle immediately. The caller of this function will batch a ref-dec
    /// with a state change.
    pub(super) fn release(&self, task: Task<S>) -> Option<Task<S>> {
        use std::mem::ManuallyDrop;

        let task = ManuallyDrop::new(task);

        self.scheduler.with(|ptr| {
            // Safety: Can only be called after initial `poll`, which is the
            // only time the field is mutated.
            match unsafe { &*ptr } {
                Some(scheduler) => scheduler.release(&*task),
                // Task was never polled
                None => None,
            }
        })
    }
}

impl<T: Future> CoreStage<T> {
    pub(super) fn with_mut<R>(&self, f: impl FnOnce(*mut Stage<T>) -> R) -> R {
        self.stage.with_mut(f)
    }

    /// Poll the future
    ///
    /// # Safety
    ///
    /// The caller must ensure it is safe to mutate the `state` field. This
    /// requires ensuring mutual exclusion between any concurrent thread that
    /// might modify the future or output field.
    ///
    /// The mutual exclusion is implemented by `Harness` and the `Lifecycle`
    /// component of the task state.
    ///
    /// `self` must also be pinned. This is handled by storing the task on the
    /// heap.
    pub(super) fn poll(&self, mut cx: Context<'_>) -> Poll<T::Output> {
        let res = {
            self.stage.with_mut(|ptr| {
                // Safety: The caller ensures mutual exclusion to the field.
                let future = match unsafe { &mut *ptr } {
                    Stage::Running(future) => future,
                    _ => unreachable!("unexpected stage"),
                };

                // Safety: The caller ensures the future is pinned.
                let future = unsafe { Pin::new_unchecked(future) };

                future.poll(&mut cx)
            })
        };

        if res.is_ready() {
            self.drop_future_or_output();
        }

        res
    }

    /// Drop the future
    ///
    /// # Safety
    ///
    /// The caller must ensure it is safe to mutate the `stage` field.
    pub(super) fn drop_future_or_output(&self) {
        // Safety: the caller ensures mutual exclusion to the field.
        unsafe {
            self.set_stage(Stage::Consumed);
        }
    }

    /// Store the task output
    ///
    /// # Safety
    ///
    /// The caller must ensure it is safe to mutate the `stage` field.
    pub(super) fn store_output(&self, output: super::Result<T::Output>) {
        // Safety: the caller ensures mutual exclusion to the field.
        unsafe {
            self.set_stage(Stage::Finished(output));
        }
    }

    /// Take the task output
    ///
    /// # Safety
    ///
    /// The caller must ensure it is safe to mutate the `stage` field.
    pub(super) fn take_output(&self) -> super::Result<T::Output> {
        use std::mem;

        self.stage.with_mut(|ptr| {
            // Safety:: the caller ensures mutual exclusion to the field.
            match mem::replace(unsafe { &mut *ptr }, Stage::Consumed) {
                Stage::Finished(output) => output,
                _ => panic!("JoinHandle polled after completion"),
            }
        })
    }

    unsafe fn set_stage(&self, stage: Stage<T>) {
        self.stage.with_mut(|ptr| *ptr = stage)
    }
}

cfg_rt_multi_thread! {
    impl Header {
        pub(crate) unsafe fn set_next(&self, next: Option<NonNull<Header>>) {
            self.queue_next.with_mut(|ptr| *ptr = next);
        }
    }
}

impl Trailer {
    pub(crate) unsafe fn set_waker(&self, waker: Option<Waker>) {
        self.waker.with_mut(|ptr| {
            *ptr = waker;
        });
    }

    pub(crate) unsafe fn will_wake(&self, waker: &Waker) -> bool {
        self.waker
            .with(|ptr| (*ptr).as_ref().unwrap().will_wake(waker))
    }

    pub(crate) fn wake_join(&self) {
        self.waker.with(|ptr| match unsafe { &*ptr } {
            Some(waker) => waker.wake_by_ref(),
            None => panic!("waker missing"),
        });
    }
}

#[test]
#[cfg(not(loom))]
fn header_lte_cache_line() {
    use std::mem::size_of;

    assert!(size_of::<Header>() <= 8 * size_of::<*const ()>());
}
