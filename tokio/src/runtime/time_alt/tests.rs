use super::*;
use crate::loom::thread;

use std::task::Context;

use futures_test::task::{new_count_waker, AwokenCount};

#[cfg(loom)]
const NUM_ITEMS: usize = 16;

#[cfg(not(loom))]
const NUM_ITEMS: usize = 64;

fn new_handle() -> (EntryHandle, AwokenCount) {
    let (waker, count) = new_count_waker();
    let entry = EntryHandle::new(0);
    _ = entry.poll(&mut Context::from_waker(&waker));
    (entry, count)
}

fn model<F: Fn() + Send + Sync + 'static>(f: F) {
    #[cfg(loom)]
    loom::model(f);

    #[cfg(not(loom))]
    f();
}

#[test]
fn wake_up_in_the_same_thread() {
    model(|| {
        let mut counts = Vec::new();

        let mut wake_queue = WakeQueue::new();
        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            counts.push(count);
            unsafe { wake_queue.push_front(hdl) }
        }
        wake_queue.wake_all();

        assert!(counts.into_iter().all(|c| c.get() == 1));
    });
}

#[test]
fn cancel_in_the_same_thread() {
    model(|| {
        let mut counts = Vec::new();
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            hdl.register_cancel_tx(cancel_tx.clone());
            counts.push(count);
            hdl.cancel();
        }

        let mut wake_queue = WakeQueue::new();
        for hdl in cancel_rx.recv_all() {
            unsafe {
                wake_queue.push_front(hdl);
            }
        }
        wake_queue.wake_all();

        assert!(counts.into_iter().all(|c| c.get() == 0));
    });
}

#[test]
fn wake_up_in_the_different_thread() {
    model(|| {
        let mut counts = Vec::new();
        let mut hdls = Vec::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            counts.push(count);
            hdls.push(hdl);
        }

        // wake up all handles in a different thread
        thread::spawn(move || {
            let mut wake_queue = WakeQueue::new();
            for hdl in hdls {
                unsafe { wake_queue.push_front(hdl) }
            }
            wake_queue.wake_all();
            assert!(counts.into_iter().all(|c| c.get() == 1));
        })
        .join()
        .unwrap();
    });
}

#[test]
fn cancel_in_the_different_thread() {
    model(|| {
        let mut counts = Vec::new();
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();
        let mut hdls = Vec::new();
        let mut hdls2 = Vec::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            hdl.register_cancel_tx(cancel_tx.clone());
            counts.push(count);
            hdls.push(hdl.clone());
            hdls2.push(hdl);
        }

        // this thread cancel all handles concurrently
        let jh = thread::spawn(move || {
            // cancel all handles
            for hdl in hdls {
                hdl.cancel();
            }
        });

        // cancellation queue concurrently
        for hdl in hdls2 {
            drop(hdl);
        }

        let mut wake_queue = WakeQueue::new();
        for hdl in cancel_rx.recv_all() {
            unsafe {
                wake_queue.push_front(hdl);
            }
        }
        wake_queue.wake_all();
        assert!(counts.into_iter().all(|c| c.get() == 0));

        jh.join().unwrap();
    })
}
