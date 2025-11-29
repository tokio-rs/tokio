use super::{CancellationQueueEntry, Entry, EntryHandle};
use crate::loom::sync::{Arc, Mutex};
use crate::util::linked_list;

type EntryList = linked_list::LinkedList<CancellationQueueEntry, Entry>;

#[derive(Debug)]
struct Inner {
    list: EntryList,
}

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
    /// - `hdl` must not in any [`super::cancellation_queue`], and also mus not in any [`super::WakeQueue`].
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

impl Sender {
    /// # Safety
    ///
    /// Behavior is undefined if any of the following conditions are violated:
    ///
    /// - `hdl` must not in any cancellation queue.
    pub(crate) unsafe fn send(&self, hdl: EntryHandle) {
        unsafe {
            self.inner.lock().push_front(hdl);
        }
    }
}

#[derive(Debug)]
pub(crate) struct Receiver {
    inner: Arc<Mutex<Inner>>,
}

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
        Receiver { inner },
    )
}

#[cfg(test)]
mod tests;
