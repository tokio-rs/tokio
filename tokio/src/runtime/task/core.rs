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
use crate::runtime::context;
use crate::runtime::task::raw::{self, Vtable};
use crate::runtime::task::state::State;
use crate::runtime::task::{Id, Schedule};
use crate::util::linked_list;

use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{Context, Poll, Waker};

/// The task cell. Contains the components of the task.
///
/// It is critical for `Header` to be the first field as the task structure will
/// be referenced by both *mut Cell and *mut Header.
///
/// Any changes to the layout of this struct _must_ also be reflected in the
/// const fns in raw.rs.
#[repr(C)]
pub(super) struct Cell<T: Future, S> {
    /// Hot task state data
    pub(super) header: Header,

    /// Either the future or output, depending on the execution stage.
    pub(super) core: Core<T, S>,

    /// Cold data
    pub(super) trailer: Trailer,
}

pub(super) struct CoreStage<T: Future> {
    stage: UnsafeCell<Stage<T>>,
}

/// The core of the task.
///
/// Holds the future or output, depending on the stage of execution.
///
/// Any changes to the layout of this struct _must_ also be reflected in the
/// const fns in raw.rs.
#[repr(C)]
pub(super) struct Core<T: Future, S> {
    /// Scheduler used to drive this future.
    pub(super) scheduler: S,

    /// The task's ID, used for populating `JoinError`s.
    pub(super) task_id: Id,

    /// Either the future or the output.
    pub(super) stage: CoreStage<T>,
}

/// Crate public as this is also needed by the pool.
#[repr(C)]
pub(crate) struct Header {
    /// Task state.
    pub(super) state: State,

    /// Pointer to next task, used with the injection queue.
    pub(super) queue_next: UnsafeCell<Option<NonNull<Header>>>,

    /// Table of function pointers for executing actions on the task.
    pub(super) vtable: &'static Vtable,

    /// This integer contains the id of the OwnedTasks or LocalOwnedTasks that
    /// this task is stored in. If the task is not in any list, should be the
    /// id of the list that it was previously in, or zero if it has never been
    /// in any list.
    ///
    /// Once a task has been bound to a list, it can never be bound to another
    /// list, even if removed from the first list.
    ///
    /// The id is not unset when removed from a list because we want to be able
    /// to read the id without synchronization, even if it is concurrently being
    /// removed from the list.
    pub(super) owner_id: UnsafeCell<u64>,

    /// The tracing ID for this instrumented task.
    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(super) tracing_id: Option<tracing::Id>,
}

unsafe impl Send for Header {}
unsafe impl Sync for Header {}

/// Cold data is stored after the future. Data is considered cold if it is only
/// used during creation or shutdown of the task.
pub(super) struct Trailer {
    /// Pointers for the linked list in the `OwnedTasks` that owns this task.
    pub(super) owned: linked_list::Pointers<Header>,
    /// Consumer task waiting on completion of this task.
    pub(super) waker: UnsafeCell<Option<Waker>>,
}

generate_addr_of_methods! {
    impl<> Trailer {
        pub(super) unsafe fn addr_of_owned(self: NonNull<Self>) -> NonNull<linked_list::Pointers<Header>> {
            &self.owned
        }
    }
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
    pub(super) fn new(future: T, scheduler: S, state: State, task_id: Id) -> Box<Cell<T, S>> {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let tracing_id = future.id();
        let result = Box::new(Cell {
            header: Header {
                state,
                queue_next: UnsafeCell::new(None),
                vtable: raw::vtable::<T, S>(),
                owner_id: UnsafeCell::new(0),
                #[cfg(all(tokio_unstable, feature = "tracing"))]
                tracing_id,
            },
            core: Core {
                scheduler,
                stage: CoreStage {
                    stage: UnsafeCell::new(Stage::Running(future)),
                },
                task_id,
            },
            trailer: Trailer {
                waker: UnsafeCell::new(None),
                owned: linked_list::Pointers::new(),
            },
        });

        #[cfg(debug_assertions)]
        {
            let trailer_addr = (&result.trailer) as *const Trailer as usize;
            let trailer_ptr = unsafe { Header::get_trailer(NonNull::from(&result.header)) };
            assert_eq!(trailer_addr, trailer_ptr.as_ptr() as usize);

            let scheduler_addr = (&result.core.scheduler) as *const S as usize;
            let scheduler_ptr =
                unsafe { Header::get_scheduler::<S>(NonNull::from(&result.header)) };
            assert_eq!(scheduler_addr, scheduler_ptr.as_ptr() as usize);

            let id_addr = (&result.core.task_id) as *const Id as usize;
            let id_ptr = unsafe { Header::get_id_ptr(NonNull::from(&result.header)) };
            assert_eq!(id_addr, id_ptr.as_ptr() as usize);
        }

        result
    }
}

impl<T: Future> CoreStage<T> {
    pub(super) fn with_mut<R>(&self, f: impl FnOnce(*mut Stage<T>) -> R) -> R {
        self.stage.with_mut(f)
    }
}

/// Set and clear the task id in the context when the future is executed or
/// dropped, or when the output produced by the future is dropped.
pub(crate) struct TaskIdGuard {
    parent_task_id: Option<Id>,
}

impl TaskIdGuard {
    fn enter(id: Id) -> Self {
        TaskIdGuard {
            parent_task_id: context::set_current_task_id(Some(id)),
        }
    }
}

impl Drop for TaskIdGuard {
    fn drop(&mut self) {
        context::set_current_task_id(self.parent_task_id);
    }
}

impl<T: Future, S: Schedule> Core<T, S> {
    /// Polls the future.
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
            self.stage.stage.with_mut(|ptr| {
                // Safety: The caller ensures mutual exclusion to the field.
                let future = match unsafe { &mut *ptr } {
                    Stage::Running(future) => future,
                    _ => unreachable!("unexpected stage"),
                };

                // Safety: The caller ensures the future is pinned.
                let future = unsafe { Pin::new_unchecked(future) };

                let _guard = TaskIdGuard::enter(self.task_id);
                future.poll(&mut cx)
            })
        };

        if res.is_ready() {
            self.drop_future_or_output();
        }

        res
    }

    /// Drops the future.
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

    /// Stores the task output.
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

    /// Takes the task output.
    ///
    /// # Safety
    ///
    /// The caller must ensure it is safe to mutate the `stage` field.
    pub(super) fn take_output(&self) -> super::Result<T::Output> {
        use std::mem;

        self.stage.stage.with_mut(|ptr| {
            // Safety:: the caller ensures mutual exclusion to the field.
            match mem::replace(unsafe { &mut *ptr }, Stage::Consumed) {
                Stage::Finished(output) => output,
                _ => panic!("JoinHandle polled after completion"),
            }
        })
    }

    unsafe fn set_stage(&self, stage: Stage<T>) {
        let _guard = TaskIdGuard::enter(self.task_id);
        self.stage.stage.with_mut(|ptr| *ptr = stage)
    }
}

cfg_rt_multi_thread! {
    impl Header {
        pub(super) unsafe fn set_next(&self, next: Option<NonNull<Header>>) {
            self.queue_next.with_mut(|ptr| *ptr = next);
        }
    }
}

impl Header {
    // safety: The caller must guarantee exclusive access to this field, and
    // must ensure that the id is either 0 or the id of the OwnedTasks
    // containing this task.
    pub(super) unsafe fn set_owner_id(&self, owner: u64) {
        self.owner_id.with_mut(|ptr| *ptr = owner);
    }

    pub(super) fn get_owner_id(&self) -> u64 {
        // safety: If there are concurrent writes, then that write has violated
        // the safety requirements on `set_owner_id`.
        unsafe { self.owner_id.with(|ptr| *ptr) }
    }

    /// Gets a pointer to the `Trailer` of the task containing this `Header`.
    ///
    /// # Safety
    ///
    /// The provided raw pointer must point at the header of a task.
    pub(super) unsafe fn get_trailer(me: NonNull<Header>) -> NonNull<Trailer> {
        let offset = me.as_ref().vtable.trailer_offset;
        let trailer = me.as_ptr().cast::<u8>().add(offset).cast::<Trailer>();
        NonNull::new_unchecked(trailer)
    }

    /// Gets a pointer to the scheduler of the task containing this `Header`.
    ///
    /// # Safety
    ///
    /// The provided raw pointer must point at the header of a task.
    ///
    /// The generic type S must be set to the correct scheduler type for this
    /// task.
    pub(super) unsafe fn get_scheduler<S>(me: NonNull<Header>) -> NonNull<S> {
        let offset = me.as_ref().vtable.scheduler_offset;
        let scheduler = me.as_ptr().cast::<u8>().add(offset).cast::<S>();
        NonNull::new_unchecked(scheduler)
    }

    /// Gets a pointer to the id of the task containing this `Header`.
    ///
    /// # Safety
    ///
    /// The provided raw pointer must point at the header of a task.
    pub(super) unsafe fn get_id_ptr(me: NonNull<Header>) -> NonNull<Id> {
        let offset = me.as_ref().vtable.id_offset;
        let id = me.as_ptr().cast::<u8>().add(offset).cast::<Id>();
        NonNull::new_unchecked(id)
    }

    /// Gets the id of the task containing this `Header`.
    ///
    /// # Safety
    ///
    /// The provided raw pointer must point at the header of a task.
    pub(super) unsafe fn get_id(me: NonNull<Header>) -> Id {
        let ptr = Header::get_id_ptr(me).as_ptr();
        *ptr
    }

    /// Gets the tracing id of the task containing this `Header`.
    ///
    /// # Safety
    ///
    /// The provided raw pointer must point at the header of a task.
    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(super) unsafe fn get_tracing_id(me: &NonNull<Header>) -> Option<&tracing::Id> {
        me.as_ref().tracing_id.as_ref()
    }
}

impl Trailer {
    pub(super) unsafe fn set_waker(&self, waker: Option<Waker>) {
        self.waker.with_mut(|ptr| {
            *ptr = waker;
        });
    }

    pub(super) unsafe fn will_wake(&self, waker: &Waker) -> bool {
        self.waker
            .with(|ptr| (*ptr).as_ref().unwrap().will_wake(waker))
    }

    pub(super) fn wake_join(&self) {
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
