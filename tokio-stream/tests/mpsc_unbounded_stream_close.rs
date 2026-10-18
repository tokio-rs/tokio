#![cfg(all(not(loom), not(target_family = "wasm")))]

use futures::{task::noop_waker_ref, Stream};
use futures_core::FusedStream;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

struct Allocator;

thread_local! {
    static ON_ALLOC: Cell<Option<Box<dyn FnOnce()>>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(callback) = ON_ALLOC.try_with(Cell::take).ok().flatten() {
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

    let mut stream = UnboundedReceiverStream::new(rx);
    let observation = Rc::new(Cell::new(None));
    let observation_on_alloc = observation.clone();
    let callback = Box::new(move || {
        // Allocation happens after the send has been accepted, before the message
        // is ready. Close and inspect the stream while that send is unfinished.
        stream.close();
        let size_hint = stream.size_hint();
        let terminated = stream.is_terminated();
        observation_on_alloc.set(Some((stream, size_hint, terminated)));
    });
    ON_ALLOC.with(|hook| hook.set(Some(callback)));
    let sent = tx.send(block_capacity);
    let unused_callback = ON_ALLOC.with(Cell::take);

    assert!(unused_callback.is_none(), "send did not allocate a block");
    sent.unwrap();
    let (mut stream, size_hint, terminated) = observation.take().unwrap();
    assert!(!terminated, "an accepted send is still unfinished");
    assert_eq!(size_hint.0, 0);
    assert!(size_hint.1.is_none_or(|upper| upper >= 1));

    let mut cx = Context::from_waker(noop_waker_ref());
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut cx),
        Poll::Ready(Some(block_capacity))
    );
    assert_eq!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Ready(None));
    assert!(stream.is_terminated());
    assert_eq!(stream.size_hint(), (0, Some(0)));
}
