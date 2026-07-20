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
    model(|| {
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();
        let mut wheel = Wheel::new();

        // Use a deadline far in the future so it can't accidentally be
        // treated as already-expired.
        let far_future = 10_000_000;
        let (hdl, count) = new_handle_with_deadline(far_future);

        // The timer is cancelled *before* it is ever inserted into a wheel,
        // i.e. before it has a `cancel_tx` to remove itself through.
        hdl.cancel();
        assert!(hdl.is_cancelled());

        // The driver now processes it, exactly as
        // `scheduler::util::time_alt::process_registration_queue` /
        // `insert_inject_timers` do for entries that haven't hit their
        // deadline yet.
        unsafe {
            wheel.insert(hdl, cancel_tx);
        }

        // The cancelled entry must NOT be occupying a slot in the wheel:
        // there is no live `Sleep`/`Timer` left that could ever remove it,
        // so if it went into the wheel it is stuck there until it naturally
        // expires or the runtime shuts down.
        assert!(
            wheel.next_expiration_time().is_none(),
            "an already-cancelled entry leaked into the wheel on insert"
        );

        // It also must not have been queued for cancellation removal, since
        // it was never actually placed in the wheel for `remove` to find.
        assert_eq!(cancel_rx.recv_all().count(), 0);
        assert_eq!(count.get(), 0);

        // `Wheel` has no `Drop` impl of its own (unlike the other queues) --
        // it relies on the driver always fully draining it via
        // `take_expired`/`shutdown_alt` before it goes away. Do the same
        // here so a leaked entry (i.e. this test failing) is reported via
        // the assertion above and not obscured by an unrelated
        // "Arc leaked" panic from loom's own leak checker.
        let mut wake_queue = WakeQueue::new();
        wheel.take_expired(u64::MAX, &mut wake_queue);
        wake_queue.wake_all();
    });
}

#[test]
/// The concurrent counterpart of the test above: this races a real
/// `Handle::cancel()` call against `Wheel::insert()` for the *same* entry
/// on two different (loom-modelled) threads, which is exactly what happens
/// when a `Sleep` created on one worker is dropped from a different thread
/// (e.g. the task got migrated) at the same moment the original worker is
/// draining its registration queue into the wheel.
///
/// This is the interleaving that a naive fix -- checking
/// `hdl.is_cancelled()` once up front before calling `Wheel::insert` --
/// would still miss: the check-then-insert would not be atomic, so a
/// `cancel()` landing in the gap could still slip an entry into the wheel
/// with no `cancel_tx` registered. The real fix makes `Wheel::insert` act
/// on the same lock-guarded answer that `register_cancel_tx` computes,
/// so there is no gap to race into.
fn cancel_races_with_insert() {
    model(|| {
        let (cancel_tx, mut cancel_rx) = cancellation_queue::new();
        let mut wheel = Wheel::new();

        let far_future = 10_000_000;
        let (hdl, count) = new_handle_with_deadline(far_future);

        let hdl2 = hdl.clone();
        let jh = thread::spawn(move || {
            hdl2.cancel();
        });

        unsafe {
            wheel.insert(hdl, cancel_tx);
        }

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

        // Drain the wheel unconditionally so this test doesn't trip loom's
        // leak checker on the (bug-exhibiting) path where the entry ended
        // up stuck in the wheel -- see the comment in
        // `insert_of_already_cancelled_entry_does_not_enter_wheel`.
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
