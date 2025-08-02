use crate::loom::sync::atomic::{AtomicU8, Ordering::*};
use crate::loom::sync::{Arc, Mutex};
use crate::{sync::AtomicWaker, util::linked_list};
use std::ptr::NonNull;
use std::sync::mpsc::Sender;
use std::task::Waker;

pub(crate) type EntryList = linked_list::LinkedList<Entry, Entry>;

/// A pure new entry, no any changes to the state.
const STATE_UNREGISTERED: u8 = 0;

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

#[derive(Debug)]
struct Inner {
    /// The tick when this entry is scheduled to expire.
    deadline: u64,

    /// The currently registered waker.
    waker: AtomicWaker,

    /// The mpsc channel used to cancel the entry.
    // Since the contention is very unlikely, we use `Mutex` here
    // for lower complexity.
    cancel_tx: Mutex<Option<Sender<Handle>>>,

    state: AtomicU8,
}

/// The entry in the timer wheel.
pub(crate) struct Entry {
    /// The pointers used by the intrusive linked list.
    pointers: linked_list::Pointers<Entry>,

    inner: Arc<Inner>,
}

generate_addr_of_methods! {
    impl<> Entry {
        unsafe fn addr_of_pointers(self: NonNull<Self>) -> NonNull<linked_list::Pointers<Entry>> {
            &self.pointers
        }
    }
}

unsafe impl linked_list::Link for Entry {
    type Handle = RawHandle;
    type Target = Entry;

    fn as_raw(hdl: &Self::Handle) -> NonNull<Self::Target> {
        hdl.ptr
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle {
        RawHandle { ptr }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        Entry::addr_of_pointers(target)
    }
}

/// Raw handle used by the intrusive linked list.
// It makes no sense to `Arc::clone()` the `Inner`
// while operating on the linked list,
// so we only use a raw pointer here.
pub(crate) struct RawHandle {
    ptr: NonNull<Entry>,
}

impl RawHandle {
    /// # Safety
    ///
    /// [`Self::ptr`] must be a valid pointer to an [`Entry`].
    pub(crate) unsafe fn upgrade(self) -> Handle {
        let inner = Arc::clone(&self.ptr.as_ref().inner);
        Handle {
            ptr: self.ptr,
            inner,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Handle {
    /// A pointer to the entry in the timer wheel.
    ptr: NonNull<Entry>,

    inner: Arc<Inner>,
}

/// Safety:
///
/// 1. [`Self::inner`] is clearly [`Send`].
/// 2. AND caller guarantees that the [`Self::drop_entry`] is only called
///    when the entry is no longer in the timer wheel and still valid.
unsafe impl Send for Handle {}

/// Safety:
///
/// 1. [`Self::inner`] is clearly [`Sync`].
/// 2. AND caller guarantees that the [`Self::drop_entry`] is only called
///    when the entry is no longer in the timer wheel and still valid.
unsafe impl Sync for Handle {}

impl Handle {
    pub(crate) fn new(deadline: u64, waker: &Waker) -> Self {
        let inner = Arc::new(Inner {
            deadline,
            waker: AtomicWaker::new(),
            cancel_tx: Mutex::new(None),
            state: AtomicU8::new(STATE_UNREGISTERED),
        });
        inner.waker.register_by_ref(waker);

        let ptr = Box::into_raw(Box::new(Entry {
            pointers: linked_list::Pointers::new(),
            inner: Arc::clone(&inner),
        }));
        // Safety: `Box::into_raw` always returns a valid pointer
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Handle { ptr, inner }
    }

    /// Wake the entry if it is already in the pending queue of the timer wheel.
    ///
    /// # Panic
    ///
    /// Panics if the entry is not transitioned to the pending state.
    pub(crate) fn wake(&self) {
        let old = self.inner.state.swap(STATE_WOKEN_UP, SeqCst);
        assert!(old == STATE_PENDING);
        self.inner.waker.wake();
    }

    /// Wake the entry if it has already elapsed before registering to the timer wheel.
    ///
    /// # Panic
    ///
    /// Panics if the entry is not in the unregistered state.
    pub(crate) fn wake_unregistered(&self) {
        let old = self.inner.state.swap(STATE_WOKEN_UP, SeqCst);
        assert!(old == STATE_UNREGISTERED);
        self.inner.waker.wake();
    }

    pub(crate) fn register_waker(&self, waker: &Waker) {
        self.inner.waker.register_by_ref(waker);
    }

    /// # Panic
    ///
    /// Panics if the entry is not in the unregistered state.
    pub(crate) fn transition_to_registered(&self, cancel_tx: Sender<Handle>) {
        {
            let mut maybe_tx = self.inner.cancel_tx.lock();
            assert!(maybe_tx.is_none(), "cancel sender already set");
            *maybe_tx = Some(cancel_tx);
            // lock is dropped here
        }
        let old = self.inner.state.swap(STATE_REGISTERED, SeqCst);
        assert_eq!(old, STATE_UNREGISTERED, "Entry not unregistered");
    }

    /// # Panic
    ///
    /// Panics if the entry is not in the registered state.
    pub(crate) fn transition_to_pending(&self, not_after: u64) -> Result<(), u64> {
        if self.inner.deadline > not_after {
            return Err(self.inner.deadline);
        }
        let old = self.inner.state.swap(STATE_PENDING, SeqCst);
        assert_eq!(old, STATE_REGISTERED, "Entry not registered");
        Ok(())
    }

    /// # Panic
    ///
    /// Panics if receiver side is closed, this is usually caused by
    /// the shutdown logic dropping the receiver side too early.
    pub(crate) fn cancel(&self) {
        let state = self.inner.state.fetch_or(0, SeqCst);
        if state & STATE_REGISTERED != 0 {
            let maybe_tx = {
                let mut lock = self.inner.cancel_tx.lock();
                lock.take()
                // lock is dropped here to avoid poisoning the Mutex
            };
            if let Some(tx) = maybe_tx {
                tx.send(self.clone())
                    .expect("cancel sender should not be closed");
            }
        }
    }

    pub(crate) fn deadline(&self) -> u64 {
        self.inner.deadline
    }

    pub(crate) fn is_registered(&self) -> bool {
        self.inner.state.fetch_or(0, SeqCst) == STATE_REGISTERED
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.inner.state.fetch_or(0, SeqCst) == STATE_PENDING
    }

    pub(crate) fn is_woken_up(&self) -> bool {
        self.inner.state.fetch_or(0, SeqCst) == STATE_WOKEN_UP
    }

    pub(crate) fn as_raw(&self) -> RawHandle {
        RawHandle { ptr: self.ptr }
    }

    pub(crate) fn as_entry_ptr(&self) -> NonNull<Entry> {
        self.ptr
    }

    /// # Safety
    ///
    /// [`Self::ptr`] must be a valid pointer to an [`Entry`].
    pub(crate) unsafe fn drop_entry(&self) {
        drop(Box::from_raw(self.ptr.as_ptr()));
    }
}
