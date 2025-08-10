//! MPSC Intrusive Linked List
//!
//! This is a highly customized implementation based on
//! Dmitry Vyukov's [Intrusive MPSC node-based queue].
//!
//! This major difference is that the [`Receiver`]
//! always returns all items in the queue,
//! instead of just one item at a time.
//!
//! [Intrusive MPSC node-based queue]: https://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue

use super::{Entry, EntryHandle};
use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{AtomicPtr, Ordering::*};
use crate::loom::sync::Arc;

use std::iter::Iterator;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ptr::{null, null_mut, NonNull};
use std::task::{RawWaker, RawWakerVTable, Waker};

fn spin_loop() {
    #[cfg(loom)]
    crate::loom::thread::yield_now();

    #[cfg(not(loom))]
    std::hint::spin_loop();
}

#[derive(Debug)]
struct Inner {
    head: UnsafeCell<NonNull<Entry>>,
    tail: AtomicPtr<Entry>,
    stub: AtomicPtr<Entry>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        // Drop the stub pointer
        let stub = NonNull::new(self.stub.load(SeqCst)).unwrap();
        drop_stub(stub);
    }
}

impl Inner {
    pub(crate) fn new() -> Self {
        let stub = new_stub();

        Self {
            head: UnsafeCell::new(NonNull::new(stub.as_ptr()).unwrap()),
            tail: AtomicPtr::new(stub.as_ptr()),
            stub: AtomicPtr::new(stub.as_ptr()),
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

        let old_tail = self.tail.swap(Arc::as_ptr(&node).cast_mut(), SeqCst);
        old_tail
            .as_ref()
            .expect("tail pointer should never be null")
            .cancel_pointer()
            .store(Arc::as_ptr(&node).cast_mut(), SeqCst);
    }

    /// # Safety
    ///
    /// Violating any of the following constraints can lead to
    /// undefined behavior:
    ///
    /// - This method must not be called concurrently.
    unsafe fn take_all(&self) -> impl Iterator<Item = EntryHandle> {
        // TODO: Using `Option` for both head and tail is a bad design,
        // imagine a case where the head is None, but the tail is Some,
        // which is very confusing.
        struct Iter {
            head: Option<NonNull<Entry>>,
            tail: Option<NonNull<Entry>>,
        }

        impl Drop for Iter {
            fn drop(&mut self) {
                for hdl in self {
                    drop(hdl)
                }
            }
        }

        impl Iterator for Iter {
            type Item = EntryHandle;

            fn next(&mut self) -> Option<Self::Item> {
                match self.head {
                    Some(head) => unsafe {
                        let atomic_next = head.as_ref().cancel_pointer();
                        let mut next = atomic_next.load(SeqCst);
                        while head != self.tail.unwrap() && next.is_null() {
                            spin_loop();
                            next = atomic_next.load(SeqCst);
                        }
                        self.head = NonNull::new(next);
                        Some(Self::Item::from(NonNull::new_unchecked(head.as_ptr())))
                    },
                    None => None,
                }
            }
        }
        let new_stub = new_stub();

        let old_tail = self.tail.swap(new_stub.as_ptr(), SeqCst);

        // At this point, `self.push` will link the new node to `new_stub`.

        // Safety: `self.head` is only access by single thread.
        let old_head = unsafe {
            self.head.with_mut(|head| {
                let old_head = *head;
                *head = NonNull::new(new_stub.as_ptr()).expect(
                    "head pointer is always equals to stub pointer, so it should never be null",
                );
                old_head
            })
        };
        let old_stub = self.stub.swap(new_stub.as_ptr(), SeqCst);

        if old_head.as_ptr() == old_tail {
            // queue is empty
            drop_stub(NonNull::new(old_stub).expect("stub pointer should never be null"));
            return Iter {
                head: None,
                tail: None,
            };
        }

        // Safety: The head pointer always equals to stub, and stub is always valid.
        let old_head_entry_cancel_pointer = unsafe { old_head.as_ref() }.cancel_pointer();
        let mut first = old_head_entry_cancel_pointer.load(SeqCst);
        while first.is_null() {
            // We enter this loop if and only if there is only one item in the queue,
            // AND the `cancel_pointer` is being set to non-null.
            spin_loop();
            first = old_head_entry_cancel_pointer.load(SeqCst);
        }

        drop_stub(NonNull::new(old_stub).expect("stub pointer should never be null"));

        // Safety:
        //
        // - We have checked `first` before.
        // - `old_tail` is is not null as `self.tail` is always not null.
        unsafe {
            Iter {
                head: Some(NonNull::new_unchecked(first)),
                tail: Some(NonNull::new_unchecked(old_tail)),
            }
        }
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
    pub(crate) unsafe fn recv_all(&mut self) -> impl Iterator<Item = EntryHandle> {
        self.inner.take_all()
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
