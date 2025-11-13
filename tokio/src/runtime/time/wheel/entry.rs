use super::cancellation_queue::Sender;
use crate::loom::sync::{Arc, Mutex};
use crate::util::linked_list;

use std::marker::PhantomPinned;
use std::ptr::NonNull;
use std::task::Waker;

pub(crate) type EntryList = linked_list::LinkedList<Entry, Entry>;

#[derive(Debug)]
enum PrivState {
    /// A pure new entry, no any changes to the state.
    Unregistered(Waker),

    /// The entry is registered to the timer wheel,
    /// but not in the pending queue of the timer wheel.
    Registered(Sender, Waker),

    /// The entry is in the pending queue of the timer wheel,
    /// and not in any wheel level, which means that
    /// the entry is reached its deadline and waiting to be woken up.
    Pending(Sender, Waker),

    /// The waker has been called, and the entry is no longer in the timer wheel
    /// (both each wheel level and the pending queue), which means that
    /// the entry is reached its deadline and woken up.
    WokenUp,

    /// The [`Handle`] has been sent to the [`Sender`].
    Cancelling(Cancelling),
}

#[derive(Debug)]
pub(crate) struct Entry {
    /// The intrusive pointers used by timer wheel.
    wheel_pointers: linked_list::Pointers<Entry>,

    /// The intrusive pointer used by cancellation queue.
    cancel_pointers: linked_list::Pointers<Entry>,

    /// The tick when this entry is scheduled to expire.
    deadline: u64,

    state: Mutex<PrivState>,

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
            entry: unsafe { Arc::from_raw(ptr.as_ptr()) },
        }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        let this = target.as_ptr();
        let field = unsafe { std::ptr::addr_of_mut!((*this).wheel_pointers) };
        unsafe { NonNull::new_unchecked(field) }
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
            entry: unsafe { Arc::from_raw(ptr.as_ptr()) },
        }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        let this = target.as_ptr();
        let field = unsafe { std::ptr::addr_of_mut!((*this).cancel_pointers) };
        unsafe { NonNull::new_unchecked(field) }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Handle {
    pub(crate) entry: Arc<Entry>,
}

impl From<&Handle> for NonNull<Entry> {
    fn from(hdl: &Handle) -> Self {
        // Safety: entry is in an `Arc`, so the pointer is valid.
        unsafe { NonNull::new_unchecked(Arc::as_ptr(&hdl.entry) as *mut Entry) }
    }
}

impl Handle {
    pub(crate) fn new(deadline: u64, waker: &Waker) -> Self {
        let entry = Arc::new(Entry {
            wheel_pointers: linked_list::Pointers::new(),
            cancel_pointers: linked_list::Pointers::new(),
            deadline,
            state: Mutex::new(PrivState::Unregistered(waker.clone())),
            _pin: PhantomPinned,
        });

        Handle { entry }
    }

    /// Wake the entry if it is already in the pending queue of the timer wheel.
    pub(crate) fn wake(&self) {
        let mut lock = self.entry.state.lock();
        match &*lock {
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (PrivState::Unregistered(..) | PrivState::Registered(..)) => {
                panic!("corrupted state: {state:#?}")
            }
            PrivState::Pending(..) => {
                let old_state = std::mem::replace(&mut *lock, PrivState::WokenUp);
                // Since state has been updated, no need to hold the lock.
                drop(lock);
                if let PrivState::Pending(_, waker, ..) = old_state {
                    waker.wake();
                } else {
                    unreachable!()
                }
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            PrivState::WokenUp => panic!("corrupted state: `State::WokenUp`"),
            // no need to wake up cancelling entry
            PrivState::Cancelling { .. } => (),
        }
    }

    /// Wake the entry if it has already elapsed before registering to the timer wheel.
    pub(crate) fn wake_unregistered(&self) {
        let mut lock = self.entry.state.lock();
        match &*lock {
            PrivState::Unregistered(_waker) => {
                let old_state = std::mem::replace(&mut *lock, PrivState::WokenUp);
                // Since state has been updated, no need to hold the lock.
                drop(lock);
                if let PrivState::Unregistered(old_waker) = old_state {
                    old_waker.wake();
                } else {
                    unreachable!()
                }
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (PrivState::Registered(..) | PrivState::WokenUp) => {
                panic!("corrupted state: {state:#?}")
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            PrivState::Pending(..) => panic!("corrupted state: State::Pending"),
            // don't wake up cancelling entries
            PrivState::Cancelling { .. } => (),
        }
    }

    pub(crate) fn register_waker(&self, waker: &Waker) {
        let mut lock = self.entry.state.lock();
        let old_waker = match &mut *lock {
            PrivState::Unregistered(old_waker) => {
                if !old_waker.will_wake(waker) {
                    Some(std::mem::replace(old_waker, waker.clone()))
                } else {
                    None
                }
            }
            PrivState::Registered(_, old_waker) => {
                if !old_waker.will_wake(waker) {
                    Some(std::mem::replace(old_waker, waker.clone()))
                } else {
                    None
                }
            }
            PrivState::Pending(_, old_waker, ..) => {
                if !old_waker.will_wake(waker) {
                    Some(std::mem::replace(old_waker, waker.clone()))
                } else {
                    None
                }
            }
            PrivState::WokenUp | PrivState::Cancelling { .. } => None, // no need to update the waker
        };

        // unlock before dropping the old waker
        drop(lock);
        drop(old_waker);
    }

    pub(crate) fn transition_to_registered(&self, cancel_tx: Sender) -> TransitionToRegistered {
        let mut lock = self.entry.state.lock();
        let state: &mut PrivState = &mut lock;

        let (new_state, ret) = match state {
            PrivState::Unregistered(waker) => {
                let new_state = PrivState::Registered(cancel_tx, waker.clone());
                (Some(new_state), TransitionToRegistered::Success)
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (PrivState::Registered(..) | PrivState::Pending(..) | PrivState::WokenUp) => {
                panic!("corrupted state: {state:#?}")
            }
            PrivState::Cancelling(cancelling) => match cancelling {
                Cancelling::Unregistered => (None, TransitionToRegistered::Cancelling),
                Cancelling::Registered | Cancelling::Pending => unreachable!(),
            },
        };

        if let Some(new_state) = new_state {
            // update the state and take back the old state
            let old_state = std::mem::replace(state, new_state);

            if let PrivState::Unregistered(waker) = old_state {
                // unlock before dropping the old waker
                drop(lock);
                drop(waker);
            }
        }

        ret
    }

    pub(crate) fn transition_to_pending(&self, not_after: u64) -> TransitionToPending {
        if self.entry.deadline > not_after {
            return TransitionToPending::NotElapsed(self.entry.deadline);
        }

        let mut lock = self.entry.state.lock();
        let state: &mut PrivState = &mut lock;

        let (new_state, ret) = match state {
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            PrivState::Unregistered(_) => panic!("corrupted state: State::Unregistered"),
            PrivState::Registered(sender, waker) => {
                let new_state = PrivState::Pending(sender.clone(), waker.clone());
                (new_state, TransitionToPending::Success)
            }
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            state @ (PrivState::Pending(..) | PrivState::WokenUp) => {
                panic!("corrupted state: {state:#?}")
            }
            PrivState::Cancelling { .. } => {
                let new_state = PrivState::Cancelling(Cancelling::Pending);
                (new_state, TransitionToPending::Cancelling)
            }
        };

        // update the state and take back the old state
        let old_state = std::mem::replace(state, new_state);

        if let PrivState::Registered(_sender, waker) = old_state {
            // unlock before dropping the old waker
            drop(lock);
            drop(waker);
        }

        ret
    }

    pub(crate) fn transition_to_cancelling(&self) {
        let mut lock = self.entry.state.lock();

        match *lock {
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            PrivState::Unregistered(_) => {
                *lock = PrivState::Cancelling(Cancelling::Unregistered);
            }
            PrivState::Registered(ref tx, _) => {
                // Safety: entry is not in any cancellation queue
                unsafe {
                    tx.send(self.clone());
                }
                *lock = PrivState::Cancelling(Cancelling::Registered);
            }
            PrivState::Pending(ref tx, _) => {
                // Safety: entry is not in any cancellation queue
                unsafe {
                    tx.send(self.clone());
                }
                *lock = PrivState::Cancelling(Cancelling::Pending);
            }
            PrivState::WokenUp => (), // dropping and waking up happen concurrently
            // don't unlock — poisoning the `Mutex` stops others from using the bad state.
            PrivState::Cancelling(..) => panic!("should not be called twice"),
        }
    }

    pub(crate) fn deadline(&self) -> u64 {
        self.entry.deadline
    }

    pub(crate) fn state(&self) -> State {
        let lock = self.entry.state.lock();
        match &*lock {
            PrivState::Unregistered(_) => State::Unregistered,
            PrivState::Registered(..) => State::Registered,
            PrivState::Pending(..) => State::Pending,
            PrivState::WokenUp => State::WokenUp,
            PrivState::Cancelling(cancelling) => State::Cancelling(*cancelling),
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        match self.state() {
            State::Pending => true,
            State::Cancelling(cancelling) => match cancelling {
                Cancelling::Unregistered => unreachable!(),
                Cancelling::Registered => false,
                Cancelling::Pending => true,
            },
            _ => false,
        }
    }

    pub(crate) fn is_woken_up(&self) -> bool {
        matches!(*self.entry.state.lock(), PrivState::WokenUp)
    }

    #[cfg(test)]
    /// Only used for unit tests.
    pub(crate) fn inner_strong_count(&self) -> usize {
        Arc::strong_count(&self.entry)
    }
}

/// The result of the [`Handle::transition_to_registered`]` method.
pub(crate) enum TransitionToRegistered {
    /// The entry is being cancelled, no need to register it.
    Success,

    /// The entry is being cancelled,
    /// no need to transition it to the registered state.
    Cancelling,
}

/// The result of the [`Handle::transition_to_pending`]` method.
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

#[derive(Clone, Copy)]
pub(crate) enum State {
    Unregistered,
    Registered,
    Pending,
    WokenUp,

    /// The [`Handle`] has been sent to the [`Sender`].
    Cancelling(Cancelling),
}

#[derive(Debug, Clone, Copy)]
/// Possible variants of the [`State::Cancelling`]
pub(crate) enum Cancelling {
    /// [`Entry`] is being cancelled, and is not in the timer wheel.
    Unregistered,

    /// [`Entry`] is being cancelled, and is registered in the timer wheel,
    /// but not in the pending list.
    Registered,

    /// [`Entry`] is being cancelled, and it registered in the timer wheel,
    /// and also in the pending list.
    Pending,
}
