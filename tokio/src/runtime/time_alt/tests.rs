use super::*;
use crate::loom::thread;

use std::task::Context;

use futures_test::task::{new_count_waker, AwokenCount};

#[cfg(loom)]
const NUM_ITEMS: usize = 16;

#[cfg(not(loom))]
const NUM_ITEMS: usize = 64;

fn new_handle() -> (EntryHandle, AwokenCount) {
    new_handle_with_deadline(0)
}

fn new_handle_with_deadline(deadline: u64) -> (EntryHandle, AwokenCount) {
    let (waker, count) = new_count_waker();
    let entry = EntryHandle::new(deadline);
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

        let mut reg_queue = RegistrationQueue::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            counts.push(count);
            unsafe {
                reg_queue.push_front(hdl);
            }
        }

        let mut wake_queue = WakeQueue::new();
        for _ in 0..NUM_ITEMS {
            if let Some(hdl) = reg_queue.pop_front() {
                unsafe {
                    wake_queue.push_front(hdl);
                }
            }
        }
        assert!(reg_queue.pop_front().is_none());
        wake_queue.wake_all();

        assert!(counts.into_iter().all(|c| c.get() == 1));
    });
}

#[test]
fn cancel_in_the_same_thread() {
    model(|| {
        let mut counts = Vec::new();
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();

        let mut reg_queue = RegistrationQueue::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            assert!(hdl.register_cancel_tx(cancel_tx.clone()));
            counts.push(count);
            unsafe {
                reg_queue.push_front(hdl.clone());
            }
            hdl.cancel();
        }

        // drain the registration queue
        while let Some(hdl) = reg_queue.pop_front() {
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
    });
}

#[test]
fn insert_of_already_cancelled_entry_does_not_enter_wheel() {
    let (cancel_tx, mut cancel_rx) = cancellation_queue::new();
    let mut wheel = Wheel::new();

    // do not expire during the test
    let far_future = 10_000_000;
    let (hdl, awoken_count) = new_handle_with_deadline(far_future);

    // cancel the timer before inserting it into the wheel
    hdl.cancel();
    assert!(hdl.is_cancelled());

    // try to insert the cancelled entry into the wheel
    unsafe {
        wheel.insert(hdl, cancel_tx);
    }

    // a cancelled entry should not be inserted into the wheel
    assert!(
        wheel.next_expiration_time().is_none(),
        "an already-cancelled entry leaked into the wheel on insert"
    );

    // It also must not have been queued for cancellation removal, since
    // it was never actually placed in the wheel for `remove` to find.
    assert_eq!(cancel_rx.recv_all().count(), 0);
    assert_eq!(awoken_count.get(), 0);

    // drain the wheel unconditionally, otherwise loom will complain
    // about the leaked entry, which confuses developers in case
    // this test fails for some other reason.
    let mut wake_queue = WakeQueue::new();
    wheel.take_expired(u64::MAX, &mut wake_queue);
    wake_queue.wake_all();
}

#[test]
fn cancel_races_with_insert() {
    model(|| {
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();
        let mut wheel = Wheel::new();

        // do not expire during the test
        let far_future = 10_000_000;
        let (hdl, count) = new_handle_with_deadline(far_future);

        let hdl2 = hdl.clone();
        let jh = thread::spawn(move || {
            // cancel the timer concurrently with insertion into the wheel
            hdl2.cancel();
        });

        // try to insert the entry into the wheel concurrently with cancellation
        unsafe {
            wheel.insert(hdl, cancel_tx);
        }

        // ensure the cancellation thread has exited
        jh.join().unwrap();

        // Whichever way the race went, the entry must end up in exactly one
        // of two consistent end states -- never "in the wheel with no way
        // to ever remove it again":
        //
        // (a) `cancel()` won the race and ran first: `insert` sees it's
        //     already cancelled and refuses to add it to the wheel.
        // (b) `insert` won the race and registered `cancel_tx` first:
        //     `cancel()` then finds `cancel_tx` and pushes the entry into
        //     the cancellation queue for later removal from the wheel.
        //
        // Either way, the entry is not left stuck in the wheel with no
        // corresponding entry in the cancellation queue.
        let cancelled_via_queue = cancel_rx.recv_all().count();
        let leaked_in_wheel = wheel.next_expiration_time().is_some();

        assert!(
            !leaked_in_wheel || cancelled_via_queue == 1,
            "entry is stuck in the wheel with no way to ever be removed"
        );
        assert_eq!(count.get(), 0, "a cancelled entry must never be woken");

        // drain the wheel unconditionally, otherwise loom will complain
        // about the leaked entry, which confuses developers in case
        // this test fails for some other reason.
        let mut wake_queue = WakeQueue::new();
        wheel.take_expired(u64::MAX, &mut wake_queue);
        wake_queue.wake_all();
    });
}

#[test]
fn wake_up_in_the_different_thread() {
    model(|| {
        let mut counts = Vec::new();

        let mut hdls = Vec::new();
        let mut reg_queue = RegistrationQueue::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            counts.push(count);
            hdls.push(hdl.clone());
            unsafe {
                reg_queue.push_front(hdl);
            }
        }

        // wake up all handles in a different thread
        thread::spawn(move || {
            let mut wake_queue = WakeQueue::new();
            for _ in 0..NUM_ITEMS {
                if let Some(hdl) = reg_queue.pop_front() {
                    unsafe {
                        wake_queue.push_front(hdl);
                    }
                }
            }
            assert!(reg_queue.pop_front().is_none());
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
        let mut reg_queue = RegistrationQueue::new();

        for _ in 0..NUM_ITEMS {
            let (hdl, count) = new_handle();
            assert!(hdl.register_cancel_tx(cancel_tx.clone()));
            counts.push(count);
            hdls.push(hdl.clone());
            unsafe {
                reg_queue.push_front(hdl);
            }
        }

        // this thread cancel all handles concurrently
        let jh = thread::spawn(move || {
            // cancel all handles
            for hdl in hdls {
                hdl.cancel();
            }
        });

        // cancellation queue concurrently
        while let Some(hdl) = reg_queue.pop_front() {
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
