use super::cancellation_queue::Sender;
use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{AtomicU8, Ordering::*};
use crate::loom::sync::Arc;
use crate::{sync::AtomicWaker, util::linked_list};

use std::marker::PhantomPinned;
use std::ptr::NonNull;
use std::task::Waker;

pub(crate) type EntryList = linked_list::LinkedList<Entry, Entry>;

/// A pure new entry, no any changes to the state.
const STATE_UNREGISTERED: u8 = 0;

/// The entry is being registered to the timer wheel,
/// and also saving the `cancel_tx` to the entry.
const STATE_BUSY_REGISTERING: u8 = 1;

/// The entry is registered to the timer wheel,
/// but not in the pending queue of the timer wheel.
const STATE_REGISTERED: u8 = 2;

/// The entry is in the pending queue of the timer wheel,
/// and not in any wheel level, which means that
/// the entry is reached its deadline and waiting to be woken up.
const STATE_PENDING: u8 = 3;

/// The waker has been called, and the entry is no longer in the timer wheel
/// (both each wheel level and the pending queue), which means that
/// the entry is reached its deadline and woken up.
const STATE_WOKEN_UP: u8 = 4;

/// The [`Handle`] has been sent to the [`Sender`].
const STATE_CANCELLING: u8 = 5;

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

    /// The mpsc channel used to cancel the entry.
    cancel_tx: UnsafeCell<Option<Sender>>,

    state: AtomicU8,

    /// Make the type `!Unpin` to prevent LLVM from emitting
    /// the `noalias` attribute for mutable references.
    ///
    /// See <https://github.com/rust-lang/rust/pull/82834>.
    _pin: PhantomPinned,
}

/// Safety: There is a field is neither `Send` nor `Sync`.
///
/// - [`Self::cancel_tx`]: This is protected by [`Self::state`].
///
/// [`cancellation_queue`]: `super::cancellation_queue`
unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

impl Drop for Entry {
    fn drop(&mut self) {
        self.cancel_tx.with_mut(|tx| {
            let maybe_tx = unsafe { &mut *tx };
            drop(maybe_tx.take());
        })
    }
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
// `impl for (Entry,)` is to avoid conflicts with the `Entry` impl,
// this enables using `Entry` in multiple intrusive lists,
// this `impl` is for `cancellation_queue`.
// Safety: `Entry` is always in an `Arc`.
unsafe impl linked_list::Link for (Entry,) {
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
            cancel_tx: UnsafeCell::new(None),
            state: AtomicU8::new(STATE_UNREGISTERED),
            _pin: PhantomPinned,
        });
        entry.waker.register_by_ref(waker);

        Handle { entry }
    }

    /// Wake the entry if it is already in the pending queue of the timer wheel.
    pub(crate) fn wake(&self) {
        match self
            .entry
            .state
            // We don't need to synchronize anything, so we can use relaxed ordering.
            .compare_exchange(STATE_PENDING, STATE_WOKEN_UP, Relaxed, Relaxed)
        {
            Ok(_) => self.entry.waker.wake(),
            Err(STATE_UNREGISTERED) => {
                panic!("entry is not registered, please call `wake_unregistered` instead")
            }
            Err(STATE_BUSY_REGISTERING) => {
                panic!("should be be called concurrently with `transition_to_registered`")
            }
            Err(STATE_REGISTERED) => panic!("should not be called on non-pending entry"),
            Err(STATE_WOKEN_UP) => panic!("should not be called on woken up entry"),
            Err(STATE_CANCELLING) => (), // no need to wake up cancelling entries
            Err(actual) => panic!("state is corrupted ({actual})"),
        }
    }

    /// Wake the entry if it has already elapsed before registering to the timer wheel.
    pub(crate) fn wake_unregistered(&self) {
        match self.entry.state.compare_exchange(
            STATE_UNREGISTERED,
            STATE_WOKEN_UP,
            Relaxed, // no need to synchronize anything
            Relaxed, // no need to synchronize anything
        ) {
            Ok(_) => self.entry.waker.wake(),
            Err(STATE_REGISTERED) => {
                panic!("entry is already registered, please call `wake` instead")
            }
            Err(STATE_BUSY_REGISTERING) => {
                panic!("should be be called concurrently with `transition_to_registered`")
            }
            Err(STATE_PENDING) => {
                panic!("entry is already pending, please call `wake` instead")
            }
            Err(STATE_WOKEN_UP) => panic!("entry is already woken up"),
            Err(STATE_CANCELLING) => (), // no need to wake up cancelling entries
            Err(actual) => panic!("state is corrupted ({actual})"),
        }
    }

    pub(crate) fn register_waker(&self, waker: &Waker) {
        self.entry.waker.register_by_ref(waker);
    }

    pub(crate) fn transition_to_registered(&self, cancel_tx: Sender) -> TransitionToRegistered {
        match self.entry.state.compare_exchange(
            STATE_UNREGISTERED,
            STATE_BUSY_REGISTERING,
            Relaxed, // no need to synchronize anything
            Relaxed, // no need to synchronize anything
        ) {
            Ok(_) => (), // successfully locked the `self.cancel_tx`
            Err(STATE_BUSY_REGISTERING) => panic!("should not be called concurrently"),
            Err(STATE_REGISTERED) => panic!("should not be called twice"),
            Err(STATE_PENDING) => panic!("entry is already pending, cannot register again"),
            Err(STATE_WOKEN_UP) => panic!("already woken up, cannot register again"),
            Err(STATE_CANCELLING) => return TransitionToRegistered::Cancelling,
            Err(actual) => panic!("state is corrupted ({actual})"),
        }

        self.entry.cancel_tx.with_mut(|tx| {
            // Safety: we have claimed the `STATE_BUSY_REGISTERING` state
            let tx = unsafe { tx.as_mut().unwrap_unchecked() };
            assert!(tx.replace(cancel_tx).is_none(), "duplicate registration");
        });

        match self.entry.state.compare_exchange(
            STATE_BUSY_REGISTERING,
            STATE_REGISTERED,
            Release, // `Release` the `cancel_tx` to other threads
            Relaxed,
        ) {
            Ok(_) => TransitionToRegistered::Success,
            Err(actual) => panic!("state is corrupted ({actual})"),
        }
    }

    pub(crate) fn transition_to_pending(&self, not_after: u64) -> TransitionToPending {
        if self.entry.deadline > not_after {
            return TransitionToPending::NotElapsed(self.entry.deadline);
        }
        match self
            .entry
            .state
            // We don't need to synchronize anything, so we can use relaxed ordering.
            .compare_exchange(STATE_REGISTERED, STATE_PENDING, Relaxed, Relaxed)
        {
            Ok(_) => TransitionToPending::Success,
            Err(STATE_UNREGISTERED) => panic!("should not be called on unregistered entry"),
            Err(STATE_BUSY_REGISTERING) => {
                panic!("should not be called concurrently with `transition_to_registered`")
            }
            Err(STATE_PENDING) => panic!("should not be called twice"),
            Err(STATE_WOKEN_UP) => panic!("should not be called on woken up entry"),
            Err(STATE_CANCELLING) => TransitionToPending::Cancelling,
            Err(actual) => panic!("state is corrupted ({actual})"),
        }
    }

    pub(crate) fn transition_to_cancelling(&self) {
        loop {
            match self.entry.state.compare_exchange(
                STATE_REGISTERED,
                STATE_CANCELLING,
                Acquire, // `Acquire` the side-effects of `transition_to_registered`
                Relaxed,
            ) {
                Ok(_) => break,
                Err(STATE_UNREGISTERED) => return, // no need to cancel unregistered entries.
                Err(STATE_BUSY_REGISTERING) => {
                    // Entry is being registered, wait for it to finish.
                    std::hint::spin_loop();
                    continue;
                }
                Err(STATE_PENDING) => return, // no need to cancel pending entries
                Err(STATE_WOKEN_UP) => return, // no need to cancel woken up entries
                Err(STATE_CANCELLING) => panic!("should not be called twice"),
                Err(actual) => panic!("state is corrupted ({actual})"),
            }
        }
        self.entry.cancel_tx.with_mut(|tx| {
            // Safety: Since previous state is `STATE_REGISTERED`,
            // this is synchronized with the `transition_to_registered` call,
            // and the `cancel_tx` should be already stored.
            let tx = unsafe { tx.as_mut().unwrap_unchecked() };
            let tx = tx.take().unwrap();
            unsafe {
                tx.send(self.clone());
            }
        });

        // No need to emit an release fence here
        // because this method will not be called twice.
    }

    pub(crate) fn deadline(&self) -> u64 {
        self.entry.deadline
    }

    pub(crate) fn is_registered(&self) -> bool {
        self.entry.state.fetch_or(0, Relaxed) == STATE_REGISTERED
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.entry.state.fetch_or(0, Relaxed) == STATE_PENDING
    }

    pub(crate) fn is_woken_up(&self) -> bool {
        self.entry.state.fetch_or(0, Relaxed) == STATE_WOKEN_UP
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
