#![cfg(all(feature = "sync", not(loom), not(target_family = "wasm")))]

use futures::task::noop_waker_ref;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

struct Allocator;

type Callback = Box<dyn FnOnce()>;

thread_local! {
    // The allocator must not register TLS destructors. The callback is taken
    // when invoked, or explicitly cleared after the send below.
    static ON_ALLOC: ManuallyDrop<Cell<Option<Callback>>> =
        const { ManuallyDrop::new(Cell::new(None)) };
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(callback) = ON_ALLOC.try_with(|hook| hook.take()).ok().flatten() {
            // Unwinding from an allocator is undefined behavior, even if a library
            // operation in the callback unexpectedly panics.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)).is_err() {
                std::process::abort();
            }
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn close_during_send() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Fill and drain the first block so the next send needs to allocate a block.
    let block_capacity = if cfg!(target_pointer_width = "64") {
        32
    } else {
        16
    };
    for value in 0..block_capacity {
        tx.send(value).unwrap();
    }
    for value in 0..block_capacity {
        assert_eq!(rx.try_recv().unwrap(), value);
    }

    let observation = Rc::new(Cell::new(None));
    let observation_on_alloc = observation.clone();
    let callback = Box::new(move || {
        // Allocation happens after the send has been accepted, before the message
        // is ready. Close and inspect the receiver while that send is unfinished.
        rx.close();
        let len = rx.len();
        let is_empty = rx.is_empty();
        observation_on_alloc.set(Some((rx, len, is_empty)));
    });
    ON_ALLOC.with(|hook| hook.set(Some(callback)));
    let sent = tx.send(block_capacity);
    let unused_callback = ON_ALLOC.with(|hook| hook.take());

    assert!(unused_callback.is_none(), "send did not allocate a block");
    sent.unwrap();
    let (mut rx, len, is_empty) = observation.take().unwrap();
    assert!(!is_empty, "an accepted send is still unfinished");
    assert_eq!(len, 1);

    let mut cx = Context::from_waker(noop_waker_ref());
    assert_eq!(rx.poll_recv(&mut cx), Poll::Ready(Some(block_capacity)));
    assert_eq!(rx.poll_recv(&mut cx), Poll::Ready(None));
    assert!(rx.is_empty());
    assert_eq!(rx.len(), 0);
}
