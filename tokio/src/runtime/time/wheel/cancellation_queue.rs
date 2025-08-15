use super::{Entry, EntryHandle};
use crate::loom::sync::{Arc, Mutex};

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ptr::NonNull;

#[derive(Debug)]
struct Inner {
    head: Option<NonNull<Entry>>,
    tail: Option<NonNull<Entry>>,
}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Send for Inner {}

/// Safety: [`Inner`] is protected by [`Mutex`].
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            while let Some(head) = self.head {
                self.head = head.as_ref().cancel_pointer().with(|p| *p);
                drop(EntryHandle::from(head));
            }
        }
    }
}

impl Inner {
    fn new() -> Self {
        Self {
            head: None,
            tail: None,
        }
    }

    /// # Safety
    ///
    /// Behavior is undefined if any of the following conditions are violated:
    ///
    /// - `hdl` must not in any cancellation queue.
    unsafe fn push_back(&mut self, hdl: EntryHandle) {
        // Since we need to access the intrusive pointer, we must not drop the entry.
        let entry = ManuallyDrop::new(hdl.into_entry());

        entry.cancel_pointer().with_mut(|p| {
            // Safety: this UnsafeCell is only accessed with the mutex locked.
            let p = unsafe { p.as_mut() }.unwrap();
            *p = None;
        });

        let entry_ptr = Arc::as_ptr(&entry).cast_mut();

        if self.head.is_none() {
            self.head = NonNull::new(entry_ptr);
            self.tail = self.head;
        } else {
            let tail = self.tail.unwrap();
            unsafe {
                tail.as_ref().cancel_pointer().with_mut(|p| {
                    *p = Some(NonNull::new(entry_ptr).unwrap());
                });
            }
            self.tail = Some(NonNull::new(entry_ptr).unwrap());
        }
    }

    fn iter(&mut self) -> impl Iterator<Item = EntryHandle> {
        let mut head = self.head.take();
        let _ = self.tail.take();

        std::iter::from_fn(move || match head {
            Some(ptr) => {
                // Safety: We wrap the `hdl` using `ManuallyDrop` in `self.push_back`,
                // so the ptr is still valid.
                head = unsafe { ptr.as_ref() }
                    .cancel_pointer()
                    // Safety: All side effects have been synchronized
                    // by the mutex.
                    .with(|p| unsafe { *p });
                let hdl = EntryHandle::from(ptr);
                Some(hdl)
            }
            None => None,
        })
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
        self.inner.lock().push_back(hdl);
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
