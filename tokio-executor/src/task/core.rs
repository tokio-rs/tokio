use crate::loom::alloc::Track;
use crate::loom::cell::CausalCell;
use crate::task::raw::{self, Vtable};
use crate::task::state::State;
use crate::task::waker::waker_ref;
use crate::task::Schedule;

use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::task::{Context, Poll, Waker};

/// The task cell. Contains the components of the task.
///
/// It is critical for `Header` to be the first field as the task structure will
/// be referenced by both *mut Cell and *mut Header.
#[repr(C)]
pub(super) struct Cell<T: Future, S: 'static> {
    /// Hot task state data
    pub(super) header: Header<S>,

    /// Either the future or output, depending on the execution stage.
    pub(super) core: Core<T>,

    /// Cold data
    pub(super) trailer: Trailer,
}

/// The core of the task.
///
/// Holds the future or output, depending on the stage of execution.
pub(super) struct Core<T: Future> {
    stage: Stage<T>,
}

/// Crate public as this is also needed by the pool.
#[repr(C)]
pub(crate) struct Header<S: 'static> {
    /// Task state
    pub(super) state: State,

    /// Pointer to the executor owned by the task
    pub(super) executor: CausalCell<Option<NonNull<S>>>,

    /// Pointer to next task, used for misc task linked lists.
    pub(crate) queue_next: UnsafeCell<*const Header<S>>,

    /// Pointer to the next task in the ownership list.
    pub(crate) owned_next: UnsafeCell<Option<NonNull<Header<S>>>>,

    /// Pointer to the previous task in the ownership list.
    pub(crate) owned_prev: UnsafeCell<Option<NonNull<Header<S>>>>,

    /// Table of function pointers for executing actions on the task.
    pub(super) vtable: &'static Vtable<S>,

    /// Used by loom to track the causality of the future. Without loom, this is
    /// unit.
    pub(super) future_causality: CausalCell<()>,
}

/// Cold data is stored after the future.
pub(super) struct Trailer {
    /// Consumer task waiting on completion of this task.
    pub(super) waker: CausalCell<MaybeUninit<Option<Waker>>>,
}

/// Either the future or the output.
enum Stage<T: Future> {
    Running(Track<T>),
    Finished(Track<super::Result<T::Output>>),
    Consumed,
}

impl<T: Future, S: Schedule> Cell<T, S> {
    /// Allocate a new task cell, containing the header, trailer, and core
    /// structures.
    pub(super) fn new(future: T, state: State) -> Box<Cell<T, S>> {
        Box::new(Cell {
            header: Header {
                state,
                executor: CausalCell::new(None),
                queue_next: UnsafeCell::new(ptr::null()),
                owned_next: UnsafeCell::new(None),
                owned_prev: UnsafeCell::new(None),
                vtable: raw::vtable::<T, S>(),
                future_causality: CausalCell::new(()),
            },
            core: Core {
                stage: Stage::Running(Track::new(future)),
            },
            trailer: Trailer {
                waker: CausalCell::new(MaybeUninit::new(None)),
            },
        })
    }
}

impl<T: Future> Core<T> {
    pub(super) fn transition_to_consumed(&mut self) {
        self.stage = Stage::Consumed
    }

    pub(super) fn poll<S>(&mut self, header: &Header<S>) -> Poll<T::Output>
    where
        S: Schedule,
    {
        let res = {
            let future = match &mut self.stage {
                Stage::Running(tracked) => tracked.get_mut(),
                _ => unreachable!("unexpected stage"),
            };

            // The future is pinned within the task. The above state transition
            // has ensured the safety of this action.
            let future = unsafe { Pin::new_unchecked(future) };

            // The waker passed into the `poll` function does not require a ref
            // count increment.
            let waker_ref = waker_ref::<T, S>(header);
            let mut cx = Context::from_waker(&*waker_ref);

            future.poll(&mut cx)
        };

        if res.is_ready() {
            self.stage = Stage::Consumed;
        }

        res
    }

    pub(super) fn store_output(&mut self, output: super::Result<T::Output>) {
        self.stage = Stage::Finished(Track::new(output));
    }

    pub(super) unsafe fn read_output(&mut self, dst: *mut Track<super::Result<T::Output>>) {
        use std::mem;

        dst.write(match mem::replace(&mut self.stage, Stage::Consumed) {
            Stage::Finished(output) => output,
            _ => unreachable!("unexpected state"),
        });
    }
}

impl<S> Header<S> {
    pub(super) fn executor(&self) -> Option<NonNull<S>> {
        unsafe { self.executor.with(|ptr| *ptr) }
    }
}
