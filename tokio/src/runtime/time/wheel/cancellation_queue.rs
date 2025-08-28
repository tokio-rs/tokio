use super::{Entry, EntryHandle};
use crate::loom::sync::{Arc, Mutex};
use crate::runtime::time::wheel::CancellationQueueEntry;
use crate::util::linked_list;

use std::marker::PhantomData;

type EntryList = linked_list::LinkedList<CancellationQueueEntry, Entry>;

#[derive(Debug)]
struct Inner {
    list: EntryList,
}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Send for Inner {}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        // consume all entries
        let _ = self.iter().count();
    }
}

impl Inner {
    fn new() -> Self {
        Self {
            list: EntryList::new(),
        }
    }

    /// # Safety
    ///
    /// Behavior is undefined if any of the following conditions are violated:
    ///
    /// - `hdl` must not in any cancellation queue.
    unsafe fn push_front(&mut self, hdl: EntryHandle) {
        self.list.push_front(hdl);
    }

    fn iter(&mut self) -> impl Iterator<Item = EntryHandle> {
        struct Iter {
            list: EntryList,
        }

        impl Drop for Iter {
            fn drop(&mut self) {
                while let Some(hdl) = self.list.pop_front() {
                    drop(hdl);
                }
            }
        }

        impl Iterator for Iter {
            type Item = EntryHandle;

            fn next(&mut self) -> Option<Self::Item> {
                self.list.pop_front()
            }
        }

        Iter {
            list: std::mem::take(&mut self.list),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Sender {
    inner: Arc<Mutex<Inner>>,
}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Send for Sender {}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Sync for Sender {}

impl Sender {
    /// # Safety
    ///
    /// Behavior is undefined if any of the following conditions are violated:
    ///
    /// - `hdl` must not in any cancellation queue.
    pub(crate) unsafe fn send(&self, hdl: EntryHandle) {
        self.inner.lock().push_front(hdl);
    }
}

#[derive(Debug)]
pub(crate) struct Receiver {
    inner: Arc<Mutex<Inner>>,

    // Technically, receiver is `Sync`, however, we only
    // need single receiver for cancellation purpose,
    // so we make it `!Sync` to prevent abusing.
    _not_sync: PhantomData<*const ()>,
}

/// Safety: [`Inner`] is protected by [`Mutex`].
// We need the `Receiver` to be `Send` because the `Core` struct for multi-thread
// runtime will be send to another thread during the shutdown.
unsafe impl Send for Receiver {}

impl Receiver {
    pub(crate) fn recv_all(&mut self) -> impl Iterator<Item = EntryHandle> {
        self.inner.lock().iter()
    }
}

pub(crate) fn new() -> (Sender, Receiver) {
    let inner = Arc::new(Mutex::new(Inner::new()));
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver {
            inner,
            _not_sync: PhantomData,
        },
    )
}

#[cfg(test)]
mod tests;
