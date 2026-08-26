#![allow(clippy::cognitive_complexity)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

use tokio::sync::broadcast;
use tokio_test::task;
use tokio_test::{
    assert_err, assert_ok, assert_pending, assert_ready, assert_ready_err, assert_ready_ok,
};

use std::sync::Arc;

macro_rules! assert_recv {
    ($e:expr) => {
        match $e.try_recv() {
            Ok(value) => value,
            Err(e) => panic!("expected recv; got = {:?}", e),
        }
    };
}

macro_rules! assert_empty {
    ($e:expr) => {
        match $e.try_recv() {
            Ok(value) => panic!("expected empty; got = {:?}", value),
            Err(broadcast::error::TryRecvError::Empty) => {}
            Err(e) => panic!("expected empty; got = {:?}", e),
        }
    };
}

macro_rules! assert_lagged {
    ($e:expr, $n:expr) => {
        match assert_err!($e) {
            broadcast::error::TryRecvError::Lagged(n) => {
                assert_eq!(n, $n);
            }
            _ => panic!("did not lag"),
        }
    };
}

macro_rules! assert_closed {
    ($e:expr) => {
        match assert_err!($e) {
            broadcast::error::TryRecvError::Closed => {}
            _ => panic!("is not closed"),
        }
    };
}

#[allow(unused)]
trait AssertSend: Send + Sync {}
impl AssertSend for broadcast::Sender<i32> {}
impl AssertSend for broadcast::Receiver<i32> {}
impl AssertSend for broadcast::WeakSender<i32> {}

#[test]
fn send_try_recv_bounded() {
    let (tx, mut rx) = broadcast::channel(16);

    assert_empty!(rx);

    let n = assert_ok!(tx.send("hello"));
    assert_eq!(n, 1);

    let val = assert_recv!(rx);
    assert_eq!(val, "hello");

    assert_empty!(rx);
}

#[test]
fn send_two_recv() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    assert_empty!(rx1);
    assert_empty!(rx2);

    let n = assert_ok!(tx.send("hello"));
    assert_eq!(n, 2);

    let val = assert_recv!(rx1);
    assert_eq!(val, "hello");

    let val = assert_recv!(rx2);
    assert_eq!(val, "hello");

    assert_empty!(rx1);
    assert_empty!(rx2);
}

#[test]
fn send_recv_bounded() {
    let (tx, mut rx) = broadcast::channel(16);

    let mut recv = task::spawn(rx.recv());

    assert_pending!(recv.poll());

    assert_ok!(tx.send("hello"));

    assert!(recv.is_woken());
    let val = assert_ready_ok!(recv.poll());
    assert_eq!(val, "hello");
}

#[test]
fn send_two_recv_bounded() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    let mut recv1 = task::spawn(rx1.recv());
    let mut recv2 = task::spawn(rx2.recv());

    assert_pending!(recv1.poll());
    assert_pending!(recv2.poll());

    assert_ok!(tx.send("hello"));

    assert!(recv1.is_woken());
    assert!(recv2.is_woken());

    let val1 = assert_ready_ok!(recv1.poll());
    let val2 = assert_ready_ok!(recv2.poll());
    assert_eq!(val1, "hello");
    assert_eq!(val2, "hello");

    drop((recv1, recv2));

    let mut recv1 = task::spawn(rx1.recv());
    let mut recv2 = task::spawn(rx2.recv());

    assert_pending!(recv1.poll());

    assert_ok!(tx.send("world"));

    assert!(recv1.is_woken());
    assert!(!recv2.is_woken());

    let val1 = assert_ready_ok!(recv1.poll());
    let val2 = assert_ready_ok!(recv2.poll());
    assert_eq!(val1, "world");
    assert_eq!(val2, "world");
}

#[test]
fn change_tasks() {
    let (tx, mut rx) = broadcast::channel(1);

    let mut recv = Box::pin(rx.recv());

    let mut task1 = task::spawn(&mut recv);
    assert_pending!(task1.poll());

    let mut task2 = task::spawn(&mut recv);
    assert_pending!(task2.poll());

    tx.send("hello").unwrap();

    assert!(task2.is_woken());
}

#[test]
fn send_slow_rx() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    {
        let mut recv2 = task::spawn(rx2.recv());

        {
            let mut recv1 = task::spawn(rx1.recv());

            assert_pending!(recv1.poll());
            assert_pending!(recv2.poll());

            assert_ok!(tx.send("one"));

            assert!(recv1.is_woken());
            assert!(recv2.is_woken());

            assert_ok!(tx.send("two"));

            let val = assert_ready_ok!(recv1.poll());
            assert_eq!(val, "one");
        }

        let val = assert_ready_ok!(task::spawn(rx1.recv()).poll());
        assert_eq!(val, "two");

        let mut recv1 = task::spawn(rx1.recv());

        assert_pending!(recv1.poll());

        assert_ok!(tx.send("three"));

        assert!(recv1.is_woken());

        let val = assert_ready_ok!(recv1.poll());
        assert_eq!(val, "three");

        let val = assert_ready_ok!(recv2.poll());
        assert_eq!(val, "one");
    }

    let val = assert_recv!(rx2);
    assert_eq!(val, "two");

    let val = assert_recv!(rx2);
    assert_eq!(val, "three");
}

#[test]
fn drop_rx_while_values_remain() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    assert_ok!(tx.send("one"));
    assert_ok!(tx.send("two"));

    assert_recv!(rx1);
    assert_recv!(rx2);

    drop(rx2);
    drop(rx1);
}

#[test]
fn lagging_rx() {
    let (tx, mut rx1) = broadcast::channel(2);
    let mut rx2 = tx.subscribe();

    assert_ok!(tx.send("one"));
    assert_ok!(tx.send("two"));

    assert_eq!("one", assert_recv!(rx1));

    assert_ok!(tx.send("three"));

    // Lagged too far
    let x = dbg!(rx2.try_recv());
    assert_lagged!(x, 1);

    // Calling again gets the next value
    assert_eq!("two", assert_recv!(rx2));

    assert_eq!("two", assert_recv!(rx1));
    assert_eq!("three", assert_recv!(rx1));

    assert_ok!(tx.send("four"));
    assert_ok!(tx.send("five"));

    assert_lagged!(rx2.try_recv(), 1);

    assert_ok!(tx.send("six"));

    assert_lagged!(rx2.try_recv(), 1);
}

#[test]
fn send_no_rx() {
    let (tx, _) = broadcast::channel(16);

    assert_err!(tx.send("hello"));

    let mut rx = tx.subscribe();

    assert_ok!(tx.send("world"));

    let val = assert_recv!(rx);
    assert_eq!("world", val);
}

#[test]
#[should_panic]
#[cfg(not(target_family = "wasm"))] // wasm currently doesn't support unwinding
fn zero_capacity() {
    broadcast::channel::<()>(0);
}

#[test]
#[should_panic]
#[cfg(not(target_family = "wasm"))] // wasm currently doesn't support unwinding
fn capacity_too_big() {
    broadcast::channel::<()>(1 + (usize::MAX >> 1));
}

#[test]
#[cfg(panic = "unwind")]
#[cfg(not(target_family = "wasm"))] // wasm currently doesn't support unwinding
fn panic_in_clone() {
    use std::panic::{self, AssertUnwindSafe};

    #[derive(Eq, PartialEq, Debug)]
    struct MyVal(usize);

    impl Clone for MyVal {
        fn clone(&self) -> MyVal {
            assert_ne!(0, self.0);
            MyVal(self.0)
        }
    }

    let (tx, mut rx) = broadcast::channel(16);

    assert_ok!(tx.send(MyVal(0)));
    assert_ok!(tx.send(MyVal(1)));

    let res = panic::catch_unwind(AssertUnwindSafe(|| {
        let _ = rx.try_recv();
    }));

    assert_err!(res);

    let val = assert_recv!(rx);
    assert_eq!(val, MyVal(1));
}

#[test]
fn dropping_tx_notifies_rx() {
    let (tx, mut rx1) = broadcast::channel::<()>(16);
    let mut rx2 = tx.subscribe();

    let tx2 = tx.clone();

    let mut recv1 = task::spawn(rx1.recv());
    let mut recv2 = task::spawn(rx2.recv());

    assert_pending!(recv1.poll());
    assert_pending!(recv2.poll());

    drop(tx);

    assert_pending!(recv1.poll());
    assert_pending!(recv2.poll());

    drop(tx2);

    assert!(recv1.is_woken());
    assert!(recv2.is_woken());

    let err = assert_ready_err!(recv1.poll());
    assert!(is_closed(err));

    let err = assert_ready_err!(recv2.poll());
    assert!(is_closed(err));
}

#[test]
fn unconsumed_messages_are_dropped() {
    let (tx, rx) = broadcast::channel(16);

    let msg = Arc::new(());

    assert_ok!(tx.send(msg.clone()));

    assert_eq!(2, Arc::strong_count(&msg));

    drop(rx);

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn single_capacity_recvs() {
    let (tx, mut rx) = broadcast::channel(1);

    assert_ok!(tx.send(1));

    assert_eq!(assert_recv!(rx), 1);
    assert_empty!(rx);
}

#[test]
fn single_capacity_recvs_after_drop_1() {
    let (tx, mut rx) = broadcast::channel(1);

    assert_ok!(tx.send(1));
    drop(tx);

    assert_eq!(assert_recv!(rx), 1);
    assert_closed!(rx.try_recv());
}

#[test]
fn single_capacity_recvs_after_drop_2() {
    let (tx, mut rx) = broadcast::channel(1);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    drop(tx);

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 2);
    assert_closed!(rx.try_recv());
}

#[test]
fn dropping_sender_does_not_overwrite() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    drop(tx);

    assert_eq!(assert_recv!(rx), 1);
    assert_eq!(assert_recv!(rx), 2);
    assert_closed!(rx.try_recv());
}

#[test]
fn lagging_receiver_recovers_after_wrap_closed_1() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));
    drop(tx);

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_closed!(rx.try_recv());
}

#[test]
fn lagging_receiver_recovers_after_wrap_closed_2() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));
    assert_ok!(tx.send(4));
    drop(tx);

    assert_lagged!(rx.try_recv(), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_eq!(assert_recv!(rx), 4);
    assert_closed!(rx.try_recv());
}

#[test]
fn lagging_receiver_recovers_after_wrap_open() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_empty!(rx);
}

#[test]
fn receiver_len_with_lagged() {
    let (tx, mut rx) = broadcast::channel(3);

    tx.send(10).unwrap();
    tx.send(20).unwrap();
    tx.send(30).unwrap();
    tx.send(40).unwrap();

    assert_eq!(rx.len(), 4);
    assert_eq!(assert_recv!(rx), 10);

    tx.send(50).unwrap();
    tx.send(60).unwrap();

    assert_eq!(rx.len(), 5);
    assert_lagged!(rx.try_recv(), 1);
}

fn is_closed(err: broadcast::error::RecvError) -> bool {
    matches!(err, broadcast::error::RecvError::Closed)
}

#[test]
fn resubscribe_points_to_tail() {
    let (tx, mut rx) = broadcast::channel(3);
    tx.send(1).unwrap();

    let mut rx_resub = rx.resubscribe();

    // verify we're one behind at the start
    assert_empty!(rx_resub);
    assert_eq!(assert_recv!(rx), 1);

    // verify we do not affect rx
    tx.send(2).unwrap();
    assert_eq!(assert_recv!(rx_resub), 2);
    tx.send(3).unwrap();
    assert_eq!(assert_recv!(rx), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_empty!(rx);

    assert_eq!(assert_recv!(rx_resub), 3);
    assert_empty!(rx_resub);
}

#[test]
fn resubscribe_lagged() {
    let (tx, mut rx) = broadcast::channel(1);
    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut rx_resub = rx.resubscribe();
    assert_lagged!(rx.try_recv(), 1);
    assert_empty!(rx_resub);

    assert_eq!(assert_recv!(rx), 2);
    assert_empty!(rx);
    assert_empty!(rx_resub);
}

#[test]
fn resubscribe_to_closed_channel() {
    let (tx, rx) = tokio::sync::broadcast::channel::<u32>(2);
    drop(tx);

    let mut rx_resub = rx.resubscribe();
    assert_closed!(rx_resub.try_recv());
}

#[test]
fn sender_len() {
    let (tx, mut rx1) = broadcast::channel(4);
    let mut rx2 = tx.subscribe();

    assert_eq!(tx.len(), 0);
    assert!(tx.is_empty());

    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();

    assert_eq!(tx.len(), 3);
    assert!(!tx.is_empty());

    assert_recv!(rx1);
    assert_recv!(rx1);

    assert_eq!(tx.len(), 3);
    assert!(!tx.is_empty());

    assert_recv!(rx2);

    assert_eq!(tx.len(), 2);
    assert!(!tx.is_empty());

    tx.send(4).unwrap();
    tx.send(5).unwrap();
    tx.send(6).unwrap();

    assert_eq!(tx.len(), 4);
    assert!(!tx.is_empty());
}

#[test]
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn sender_len_random() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    for _ in 0..1000 {
        match rand::random_range(0..4) {
            0 => {
                let _ = rx1.try_recv();
            }
            1 => {
                let _ = rx2.try_recv();
            }
            _ => {
                tx.send(0).unwrap();
            }
        }

        let expected_len = usize::min(usize::max(rx1.len(), rx2.len()), 16);
        assert_eq!(tx.len(), expected_len);
    }
}

#[test]
fn send_in_waker_drop() {
    use futures::task::ArcWake;
    use std::future::Future;
    use std::task::Context;

    struct SendOnDrop(broadcast::Sender<()>);

    impl Drop for SendOnDrop {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    impl ArcWake for SendOnDrop {
        fn wake_by_ref(_arc_self: &Arc<Self>) {}
    }

    // Test if there is no deadlock when replacing the old waker.

    let (tx, mut rx) = broadcast::channel(16);

    let mut fut = Box::pin(async {
        let _ = rx.recv().await;
    });

    // Store our special waker in the receiving future.
    let waker = futures::task::waker(Arc::new(SendOnDrop(tx)));
    let mut cx = Context::from_waker(&waker);
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    drop(waker);

    // Second poll shouldn't deadlock.
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    let _ = fut.as_mut().poll(&mut cx);

    // Test if there is no deadlock when calling waker.wake().

    let (tx, mut rx) = broadcast::channel(16);

    let mut fut = Box::pin(async {
        let _ = rx.recv().await;
    });

    // Store our special waker in the receiving future.
    let waker = futures::task::waker(Arc::new(SendOnDrop(tx.clone())));
    let mut cx = Context::from_waker(&waker);
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    drop(waker);

    // Shouldn't deadlock.
    let _ = tx.send(());
}

#[tokio::test]
async fn receiver_recv_is_cooperative() {
    let (tx, mut rx) = broadcast::channel(8);

    tokio::select! {
        biased;
        _ = async {
            loop {
                assert!(tx.send(()).is_ok());
                assert!(rx.recv().await.is_ok());
            }
        } => {},
        _ = tokio::task::yield_now() => {},
    }
}

#[test]
fn broadcast_sender_closed() {
    let (tx, rx) = broadcast::channel::<()>(1);
    let rx2 = tx.subscribe();

    let mut task = task::spawn(tx.closed());
    assert_pending!(task.poll());

    drop(rx);
    assert!(!task.is_woken());
    assert_pending!(task.poll());

    drop(rx2);
    assert!(task.is_woken());
    assert_ready!(task.poll());
}

#[test]
fn broadcast_sender_closed_with_extra_subscribe() {
    let (tx, rx) = broadcast::channel::<()>(1);
    let rx2 = tx.subscribe();

    let mut task = task::spawn(tx.closed());
    assert_pending!(task.poll());

    drop(rx);
    assert!(!task.is_woken());
    assert_pending!(task.poll());

    drop(rx2);
    assert!(task.is_woken());

    let rx3 = tx.subscribe();
    assert_pending!(task.poll());

    drop(rx3);
    assert!(task.is_woken());
    assert_ready!(task.poll());

    let mut task2 = task::spawn(tx.closed());
    assert_ready!(task2.poll());

    let rx4 = tx.subscribe();
    let mut task3 = task::spawn(tx.closed());
    assert_pending!(task3.poll());

    drop(rx4);
    assert!(task3.is_woken());
    assert_ready!(task3.poll());
}

#[tokio::test]
async fn broadcast_sender_new_must_be_closed() {
    let capacity = 1;
    let tx: broadcast::Sender<()> = broadcast::Sender::new(capacity);

    let mut task = task::spawn(tx.closed());
    assert_ready!(task.poll());

    let _rx = tx.subscribe();

    let mut task2 = task::spawn(tx.closed());
    assert_pending!(task2.poll());
}

// --- Lagging semantics -------------------------------------------------------
//
// Capacity is rounded up to the next power of two. That buffer length is the
// maximum number of messages retained; exceeding it overwrites the oldest
// message and causes RecvError::Lagged / TryRecvError::Lagged on the slow
// receiver. After Lagged, the cursor points at the oldest retained message.

/// A slow receiver that stays within the ring buffer receives every message
/// in order; lagging only begins once the buffer wraps past the receiver.
#[test]
fn slow_receiver_within_capacity_does_not_lag() {
    // capacity 2 → buffer length 2
    let (tx, mut slow) = broadcast::channel(2);
    let mut fast = tx.subscribe();

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));

    // Fast keeps up; slow has not read yet but both messages are still retained.
    assert_eq!(assert_recv!(fast), 1);
    assert_eq!(assert_recv!(fast), 2);
    assert_empty!(fast);

    assert_eq!(assert_recv!(slow), 1);
    assert_eq!(assert_recv!(slow), 2);
    assert_empty!(slow);
}

/// When sends overwrite the oldest retained values, the slow receiver sees
/// Lagged(n) with the correct miss count, then resumes from the oldest
/// retained message in send order.
#[test]
fn capacity_overflow_reports_lagged_then_oldest_retained() {
    // capacity 2 → buffer length 2; third send overwrites the first.
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(10));
    assert_ok!(tx.send(20));
    assert_ok!(tx.send(30));

    assert_lagged!(rx.try_recv(), 1);
    // After Lagged, cursor is at oldest retained (20).
    assert_eq!(assert_recv!(rx), 20);
    assert_eq!(assert_recv!(rx), 30);
    assert_empty!(rx);
}

/// Lagged(n) counts every overwritten message since the receiver's previous
/// cursor, not merely "at least one".
#[test]
fn lagged_count_matches_number_of_overwritten_messages() {
    // capacity 4 → buffer length 4
    let (tx, mut rx) = broadcast::channel(4);

    for i in 1..=4 {
        assert_ok!(tx.send(i));
    }
    // Still within capacity: no lag.
    assert_eq!(assert_recv!(rx), 1);

    // Send four more without reading: overwrites 2, 3, 4, and then the slot
    // that held 1 (already consumed). Receiver still owed 2,3,4 — all gone —
    // so it missed 3 messages; oldest retained is 5.
    for i in 5..=8 {
        assert_ok!(tx.send(i));
    }

    assert_lagged!(rx.try_recv(), 3);
    assert_eq!(assert_recv!(rx), 5);
    assert_eq!(assert_recv!(rx), 6);
    assert_eq!(assert_recv!(rx), 7);
    assert_eq!(assert_recv!(rx), 8);
    assert_empty!(rx);
}

/// Non-power-of-two capacity is rounded up; lag detection uses the rounded
/// buffer length (e.g. capacity 3 → buffer 4).
#[test]
fn lag_uses_power_of_two_buffer_length() {
    // capacity 3 → buffer length 4
    let (tx, mut rx) = broadcast::channel(3);

    for i in 1..=4 {
        assert_ok!(tx.send(i));
    }
    // Four messages fit in the rounded buffer; no lag yet.
    assert_eq!(rx.len(), 4);
    assert_eq!(assert_recv!(rx), 1);

    // After reading 1, next points at 2. Buffer holds 2,3,4,5 — still no lag.
    assert_ok!(tx.send(5));
    assert_eq!(rx.len(), 4);

    // Overwrites 2; receiver still wants 2 → Lagged(1), resume at 3.
    assert_ok!(tx.send(6));
    assert_eq!(rx.len(), 5);
    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 3);
    assert_eq!(assert_recv!(rx), 4);
    assert_eq!(assert_recv!(rx), 5);
    assert_eq!(assert_recv!(rx), 6);
    assert_empty!(rx);
}

/// Async `recv` reports the same Lagged semantics as `try_recv`.
#[tokio::test]
async fn async_recv_lagged_then_resumes_from_oldest_retained() {
    use broadcast::error::RecvError;

    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(10));
    assert_ok!(tx.send(20));
    assert_ok!(tx.send(30));

    assert!(matches!(rx.recv().await, Err(RecvError::Lagged(1))));
    assert_eq!(rx.recv().await.unwrap(), 20);
    assert_eq!(rx.recv().await.unwrap(), 30);
}

/// A receiver that lags, catches up, then lags again reports a fresh miss
/// count based on the new gap only.
#[test]
fn lag_catch_up_then_lag_again() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_empty!(rx);

    // Fully caught up; another overflow lags again from a clean cursor.
    assert_ok!(tx.send(4));
    assert_ok!(tx.send(5));
    assert_ok!(tx.send(6));

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 5);
    assert_eq!(assert_recv!(rx), 6);
    assert_empty!(rx);
}

/// Only the slow receiver lags; a caught-up receiver is unaffected.
#[test]
fn lag_is_per_receiver() {
    let (tx, mut slow) = broadcast::channel(2);
    let mut fast = tx.subscribe();

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));

    assert_eq!(assert_recv!(fast), 1);
    assert_eq!(assert_recv!(fast), 2);

    assert_ok!(tx.send(3));
    assert_ok!(tx.send(4));
    // Fast has read through 2; buffer holds 3,4 — no lag.
    assert_eq!(assert_recv!(fast), 3);
    assert_eq!(assert_recv!(fast), 4);

    // Slow never read; 1 and 2 were overwritten → Lagged(2), oldest is 3.
    assert_lagged!(slow.try_recv(), 2);
    assert_eq!(assert_recv!(slow), 3);
    assert_eq!(assert_recv!(slow), 4);
    assert_empty!(slow);
    assert_empty!(fast);
}

/// If the receiver lags and more messages are sent before it reads the
/// Lagged error's "resume" position, a second Lagged reflects the additional
/// overwrites since the cursor was advanced.
#[test]
fn lag_again_before_reading_retained_messages() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));

    // First lag: miss 1, cursor advances to oldest retained (value 2).
    assert_lagged!(rx.try_recv(), 1);

    // Before reading 2/3, send enough to overwrite them (and one more).
    assert_ok!(tx.send(4));
    assert_ok!(tx.send(5));
    assert_ok!(tx.send(6));

    // Cursor was at value 2; values 2, 3, and 4 are gone; oldest retained is 5.
    assert_lagged!(rx.try_recv(), 3);
    assert_eq!(assert_recv!(rx), 5);
    assert_eq!(assert_recv!(rx), 6);
    assert_empty!(rx);
}

/// With capacity 1 (buffer length 1), every send after the first unread one
/// causes a lag of exactly one when the receiver finally reads.
#[test]
fn single_slot_capacity_lag_semantics() {
    let (tx, mut rx) = broadcast::channel(1);

    assert_ok!(tx.send(1));
    assert_eq!(assert_recv!(rx), 1);

    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));

    assert_lagged!(rx.try_recv(), 1);
    assert_eq!(assert_recv!(rx), 3);
    assert_empty!(rx);
}

/// `len` after lagging still counts from the old cursor until Lagged is
/// observed and the cursor advances; then `len` reflects retained messages.
#[test]
fn receiver_len_after_lag_error_advances_cursor() {
    let (tx, mut rx) = broadcast::channel(2);

    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));
    assert_ok!(tx.send(3));
    assert_ok!(tx.send(4));

    // Missed 1 and 2; buffer holds 3,4. len counts from old cursor.
    assert_eq!(rx.len(), 4);
    assert_lagged!(rx.try_recv(), 2);
    // Cursor now at oldest retained (3); two messages remain.
    assert_eq!(rx.len(), 2);
    assert_eq!(assert_recv!(rx), 3);
    assert_eq!(rx.len(), 1);
    assert_eq!(assert_recv!(rx), 4);
    assert_eq!(rx.len(), 0);
}
