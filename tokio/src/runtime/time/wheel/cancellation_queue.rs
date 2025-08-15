//! MPSC Intrusive Linked List
//!
//! This implementation is based on Dmitry Vyukov's [Intrusive MPSC node-based queue].
//!
//! [Intrusive MPSC node-based queue]: https://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue

use super::{Entry, EntryHandle};
use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{AtomicPtr, Ordering::*};
use crate::loom::sync::Arc;

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ptr::{null, null_mut, NonNull};
use std::task::{RawWaker, RawWakerVTable, Waker};

#[derive(Debug)]
struct Inner {
    head: AtomicPtr<Entry>,
    tail: UnsafeCell<NonNull<Entry>>,
    stub: NonNull<Entry>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            while let Some(hdl) = self.try_recv() {
                drop(hdl);
            }
        }
        drop_stub(self.stub);
    }
}

impl Inner {
    pub(crate) fn new() -> Self {
        let stub = new_stub();

        Self {
            head: AtomicPtr::new(stub.as_ptr()),
            tail: UnsafeCell::new(stub),
            stub,
        }
    }

    /// # Safety
    ///
    /// Violating any of the following constraints can lead to
    /// undefined behavior:
    ///
    /// - `hdl` must not be in any queue.
    unsafe fn push(&self, hdl: EntryHandle) {
        // Since all items in the queue must be alive until they are removed,
        // so we should not decrease the reference count.
        let node = ManuallyDrop::new(hdl.into_entry());

        let next = node.cancel_pointer();
        next.store(null_mut(), SeqCst);

        let old_head = self.head.swap(Arc::as_ptr(&node).cast_mut(), SeqCst);
        old_head
            .as_ref()
            .expect("head pointer should never be null")
            .cancel_pointer()
            .store(Arc::as_ptr(&node).cast_mut(), SeqCst);
    }

    /// # Safety
    ///
    /// Violating any of the following constraints can lead to
    /// undefined behavior:
    ///
    /// - This method must not be called concurrently.
    unsafe fn try_recv(&self) -> Option<EntryHandle> {
        let mut tail = self.tail.with(|t| *t);
        let mut next = tail.as_ref().cancel_pointer().load(SeqCst);
        if tail == self.stub {
            if next.is_null() {
                return None;
            }

            self.tail.with_mut(|t| {
                *t = NonNull::new(next).unwrap();
            });
            tail = NonNull::new(next).unwrap();
            next = next.as_ref().unwrap().cancel_pointer().load(SeqCst);
        }

        if !next.is_null() {
            self.tail.with_mut(|t| {
                *t = NonNull::new(next).unwrap();
            });
            return Some(EntryHandle::from(tail));
        }

        let head = self.head.load(SeqCst);
        if tail.as_ptr() != head {
            return None;
        }

        self.push(EntryHandle::from(self.stub));
        next = tail.as_ref().cancel_pointer().load(SeqCst);

        if !next.is_null() {
            self.tail.with_mut(|t| {
                *t = NonNull::new(next).unwrap();
            });
            return Some(EntryHandle::from(tail));
        }

        None
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Sender {
    inner: Arc<Inner>,
}

/// Safety: [`Sender`] is protected by [`AtomicPtr`]
unsafe impl Send for Sender {}

/// Safety: [`Sender`] is protected by [`AtomicPtr`]
unsafe impl Sync for Sender {}

impl Sender {
    pub(crate) unsafe fn send(&self, hdl: EntryHandle) {
        self.inner.push(hdl);
    }
}

#[derive(Debug)]
pub(crate) struct Receiver {
    inner: Arc<Inner>,

    // make sure Receiver is `!Sync`
    _p: PhantomData<*const ()>,
}

/// Safety: [`Receiver`] can only be accessed from a single thread.
unsafe impl Send for Receiver {}

impl Receiver {
    pub(crate) unsafe fn try_recv(&mut self) -> Option<EntryHandle> {
        self.inner.try_recv()
    }
}

pub(crate) fn new() -> (Sender, Receiver) {
    let inner = Arc::new(Inner::new());
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver {
            inner,
            _p: PhantomData,
        },
    )
}

fn new_stub() -> NonNull<Entry> {
    let hdl = EntryHandle::new(0, &noop_waker());
    let ptr = Arc::into_raw(hdl.into_entry());
    NonNull::new(ptr.cast_mut()).expect("stub pointer should never be null")
}

fn drop_stub(stub: NonNull<Entry>) {
    let hdl = EntryHandle::from(stub);
    drop(hdl);
}

// The following noop waker implementation is from crate `futures`.
// https://docs.rs/futures/latest/futures/

unsafe fn noop_clone(_data: *const ()) -> RawWaker {
    noop_raw_waker()
}

unsafe fn noop(_data: *const ()) {}

const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);

const fn noop_raw_waker() -> RawWaker {
    RawWaker::new(null(), &NOOP_WAKER_VTABLE)
}

fn noop_waker() -> Waker {
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

#[cfg(test)]
mod tests;
