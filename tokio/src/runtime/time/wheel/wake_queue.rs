use super::{Entry, EntryHandle, EntryState};
use crate::runtime::time::wheel::WakeQueueEntry;
use crate::util::linked_list;

type EntryList = linked_list::LinkedList<WakeQueueEntry, Entry>;

/// A queue of entries that need to be woken up.
#[derive(Debug)]
pub(crate) struct WakeQueue {
    list: EntryList,
}

impl Drop for WakeQueue {
    fn drop(&mut self) {
        // drain all entries without waking them up
        while let Some(hdl) = self.list.pop_front() {
            drop(hdl);
        }
    }
}

impl WakeQueue {
    pub(crate) fn new() -> Self {
        Self {
            list: EntryList::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// # Safety
    ///
    /// Behavior is undefined if any of the following conditions are violated:
    ///
    /// - `hdl` must not in any [`super::cancellation_queue`], and also mus not in any [`WakeQueue`].
    pub(crate) unsafe fn push_front(&mut self, hdl: EntryHandle) {
        self.list.push_front(hdl);
    }

    /// Wakes all entries in the wake queue.
    ///
    /// # Panics
    ///
    /// This function panics on any of the following conditions:
    ///
    /// - The entry state is in-consistent (i.e., `WokenUp` state in the wake queue).
    /// - The waker panics while waking the entry.
    pub(crate) fn wake_all(mut self) {
        while let Some(hdl) = self.list.pop_front() {
            match hdl.state() {
                EntryState::Unregistered => hdl.wake_unregistered(),
                state @ (EntryState::Registered | EntryState::Pending) => {
                    panic!("corrupted state: {state:#?}");
                }
                EntryState::WakingUp => hdl.wake(),
                // cancellation happens concurrently, no need to wake
                EntryState::Cancelling(_) => (),
                EntryState::WokenUp => panic!("corrupted state: woken up entry in wake queue"),
            }
        }
    }
}
