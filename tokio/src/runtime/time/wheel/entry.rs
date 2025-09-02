use super::cancellation_queue::Sender;
use crate::loom::sync::{Arc, Mutex};
use crate::{sync::AtomicWaker, util::linked_list};

use std::marker::PhantomPinned;
use std::ptr::NonNull;
use std::task::Waker;

pub(crate) type EntryList = linked_list::LinkedList<Entry, Entry>;

#[derive(Debug)]
enum State {
    /// A pure new entry, no any changes to the state.
    Unregistered,

    /// The entry is registered to the timer wheel,
    /// but not in the pending queue of the timer wheel.
    Registered(Sender),

    /// The entry is in the pending queue of the timer wheel,
    /// and not in any wheel level, which means that
    /// the entry is reached its deadline and waiting to be woken up.
    Pending,

    /// The waker has been called, and the entry is no longer in the timer wheel
    /// (both each wheel level and the pending queue), which means that
    /// the entry is reached its deadline and woken up.
    WokenUp,

    /// The [`Handle`] has been sent to the [`Sender`].
    Cancelling,
}

#[derive(Debug)]
pub(crate) struct Entry {
    /// The intrusive pointers used by timer wheel.
    wheel_pointers: linked_list::Pointers<Entry>,

    /// The intrusive pointer used by cancellation queue.
    cancel_pointers: linked_list::Pointers<Entry>,

    /// The tick when this entry is scheduled to expire.
    deadline: u64,

    /// The currently registered waker.
    waker: AtomicWaker,

    state: Mutex<State>,

    /// Make the type `!Unpin` to prevent LLVM from emitting
    /// the `noalias` attribute for mutable references.
    ///
    /// See <https://github.com/rust-lang/rust/pull/82834>.
    _pin: PhantomPinned,
}

// Safety: `Entry` is always in an `Arc`.
unsafe impl linked_list::Link for Entry {
    type Handle = Handle;
    type Target = Entry;

    fn as_raw(hdl: &Self::Handle) -> NonNull<Self::Target> {
        unsafe { NonNull::new_unchecked(Arc::as_ptr(&hdl.entry).cast_mut()) }
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle {
        Handle {
            entry: Arc::from_raw(ptr.as_ptr()),
        }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        let this = target.as_ptr();
        let field = std::ptr::addr_of_mut!((*this).wheel_pointers);
        NonNull::new_unchecked(field)
    }
}

/// An ZST to allow [`super::cancellation_queue`] to utilize the [`Entry::cancel_pointers`]
/// by impl [`linked_list::Link`] as we cannot impl it on [`Entry`]
/// directly due to the conflicting implementations used by [`Entry::wheel_pointers`].
///
/// This type should never be constructed.
pub(super) struct CancellationQueueEntry;

// Safety: `Entry` is always in an `Arc`.
unsafe impl linked_list::Link for CancellationQueueEntry {
    type Handle = Handle;
    type Target = Entry;

    fn as_raw(hdl: &Self::Handle) -> NonNull<Self::Target> {
        unsafe { NonNull::new_unchecked(Arc::as_ptr(&hdl.entry).cast_mut()) }
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle {
        Handle {
            entry: Arc::from_raw(ptr.as_ptr()),
        }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        let this = target.as_ptr();
        let field = std::ptr::addr_of_mut!((*this).cancel_pointers);
        NonNull::new_unchecked(field)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Handle {
    pub(crate) entry: Arc<Entry>,
}

impl From<Handle> for NonNull<Entry> {
    fn from(handle: Handle) -> NonNull<Entry> {
        let ptr = Arc::as_ptr(&handle.entry);
        unsafe { NonNull::new_unchecked(ptr.cast_mut()) }
    }
}

impl Handle {
    pub(crate) fn new(deadline: u64, waker: &Waker) -> Self {
        let entry = Arc::new(Entry {
            wheel_pointers: linked_list::Pointers::new(),
            cancel_pointers: linked_list::Pointers::new(),
            deadline,
            waker: AtomicWaker::new(),
            state: Mutex::new(State::Unregistered),
            _pin: PhantomPinned,
        });
        entry.waker.register_by_ref(waker);

        Handle { entry }
    }

    /// Wake the entry if it is already in the pending queue of the timer wheel.
    pub(crate) fn wake(&self) {
        let mut lock = self.entry.state.lock();
        match &*lock {
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (State::Unregistered | State::Registered(_)) => {
                panic!("corrupted state: {state:#?}")
            }
            State::Pending => {
                *lock = State::WokenUp;
                // Since state has been updated, no need to hold the lock.
                drop(lock);
                self.entry.waker.wake();
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            State::WokenUp => panic!("corrupted state: `State::WokenUp`"),
            // no need to wake up cancelling entry
            State::Cancelling => (),
        }
    }

    /// Wake the entry if it has already elapsed before registering to the timer wheel.
    pub(crate) fn wake_unregistered(&self) {
        let mut lock = self.entry.state.lock();
        match &*lock {
            State::Unregistered => {
                *lock = State::WokenUp;
                // Since state has been updated, no need to hold the lock.
                drop(lock);
                self.entry.waker.wake();
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (State::Registered(_) | State::WokenUp) => {
                panic!("corrupted state: {state:#?}")
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            State::Pending => panic!("corrupted state: State::Pending"),
            // don't wake up cancelling entries
            State::Cancelling => (),
        }
    }

    pub(crate) fn register_waker(&self, waker: &Waker) {
        self.entry.waker.register_by_ref(waker);
    }

    pub(crate) fn transition_to_registered(&self, cancel_tx: Sender) -> TransitionToRegistered {
        let mut lock = self.entry.state.lock();
        match &*lock {
            State::Unregistered => {
                *lock = State::Registered(cancel_tx);
                TransitionToRegistered::Success
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (State::Registered(_) | State::Pending | State::WokenUp) => {
                panic!("corrupted state: {state:#?}")
            }
            State::Cancelling => TransitionToRegistered::Cancelling,
        }
    }

    pub(crate) fn transition_to_pending(&self, not_after: u64) -> TransitionToPending {
        if self.entry.deadline > not_after {
            return TransitionToPending::NotElapsed(self.entry.deadline);
        }

        let mut lock = self.entry.state.lock();
        match &*lock {
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            State::Unregistered => panic!("corrupted state: State::Unregistered"),
            State::Registered(_) => {
                *lock = State::Pending;
                TransitionToPending::Success
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (State::Pending | State::WokenUp) => panic!("corrupted state: {state:#?}"),
            State::Cancelling => TransitionToPending::Cancelling,
        }
    }

    pub(crate) fn transition_to_cancelling(&self) {
        let mut lock = self.entry.state.lock();

        match *lock {
            State::Unregistered => *lock = State::Cancelling,
            State::Registered(ref tx) => {
                // Safety: entry is not in any cancellation queue
                unsafe {
                    tx.send(self.clone());
                }
                *lock = State::Cancelling;
            }
            // no need to cancel a pending or woken up entry
            State::Pending | State::WokenUp => *lock = State::Cancelling,
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            State::Cancelling => panic!("should not be called twice"),
        }
    }

    pub(crate) fn deadline(&self) -> u64 {
        self.entry.deadline
    }

    pub(crate) fn is_registered(&self) -> bool {
        matches!(*self.entry.state.lock(), State::Registered(_))
    }

    pub(crate) fn is_pending(&self) -> bool {
        matches!(*self.entry.state.lock(), State::Pending)
    }

    pub(crate) fn is_woken_up(&self) -> bool {
        matches!(*self.entry.state.lock(), State::WokenUp)
    }

    #[cfg(test)]
    /// Only used for unit tests.
    pub(crate) fn inner_strong_count(&self) -> usize {
        Arc::strong_count(&self.entry)
    }
}

/// An error returned when trying to transition
/// an being cancelled entry to the registered state.
pub(crate) enum TransitionToRegistered {
    /// The entry is being cancelled, no need to register it.
    Success,

    /// The entry is being cancelled,
    /// no need to transition it to the registered state.
    Cancelling,
}

/// An result of the `transition_to_pending` method.
pub(crate) enum TransitionToPending {
    /// The entry was successfully transitioned
    /// to the pending state.
    Success,

    /// The entry doesn't reached its deadline yet,
    /// and the tick when it should be woken up is returned.
    NotElapsed(u64),

    /// The entry is being cancelled,
    /// no need to transition it to the pending state.
    Cancelling,
}
