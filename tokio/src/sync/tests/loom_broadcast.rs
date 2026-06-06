use crate::sync::broadcast;
use crate::sync::broadcast::error::RecvError::{Closed, Lagged};

use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;
use tokio_test::{assert_err, assert_ok, task};

#[test]
fn broadcast_send() {
    loom::model(|| {
        let (tx1, mut rx) = broadcast::channel(2);
        let tx1 = Arc::new(tx1);
        let tx2 = tx1.clone();

        let th1 = thread::spawn(move || {
            block_on(async {
                assert_ok!(tx1.send("one"));
                assert_ok!(tx1.send("two"));
                assert_ok!(tx1.send("three"));
            });
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                assert_ok!(tx2.send("eins"));
                assert_ok!(tx2.send("zwei"));
                assert_ok!(tx2.send("drei"));
            });
        });

        block_on(async {
            let mut num = 0;
            loop {
                match rx.recv().await {
                    Ok(_) => num += 1,
                    Err(Closed) => break,
                    Err(Lagged(n)) => num += n as usize,
                }
            }
            assert_eq!(num, 6);
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

// An `Arc` is used as the value in order to detect memory leaks.
#[test]
fn broadcast_two() {
    loom::model(|| {
        let (tx, mut rx1) = broadcast::channel::<Arc<&'static str>>(16);
        let mut rx2 = tx.subscribe();

        let th1 = thread::spawn(move || {
            block_on(async {
                let v = assert_ok!(rx1.recv().await);
                assert_eq!(*v, "hello");

                let v = assert_ok!(rx1.recv().await);
                assert_eq!(*v, "world");

                match assert_err!(rx1.recv().await) {
                    Closed => {}
                    _ => panic!(),
                }
            });
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                let v = assert_ok!(rx2.recv().await);
                assert_eq!(*v, "hello");

                let v = assert_ok!(rx2.recv().await);
                assert_eq!(*v, "world");

                match assert_err!(rx2.recv().await) {
                    Closed => {}
                    _ => panic!(),
                }
            });
        });

        assert_ok!(tx.send(Arc::new("hello")));
        assert_ok!(tx.send(Arc::new("world")));
        drop(tx);

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

#[test]
fn broadcast_wrap() {
    loom::model(|| {
        let (tx, mut rx1) = broadcast::channel(2);
        let mut rx2 = tx.subscribe();

        let th1 = thread::spawn(move || {
            block_on(async {
                let mut num = 0;

                loop {
                    match rx1.recv().await {
                        Ok(_) => num += 1,
                        Err(Closed) => break,
                        Err(Lagged(n)) => num += n as usize,
                    }
                }

                assert_eq!(num, 3);
            });
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                let mut num = 0;

                loop {
                    match rx2.recv().await {
                        Ok(_) => num += 1,
                        Err(Closed) => break,
                        Err(Lagged(n)) => num += n as usize,
                    }
                }

                assert_eq!(num, 3);
            });
        });

        assert_ok!(tx.send("one"));
        assert_ok!(tx.send("two"));
        assert_ok!(tx.send("three"));

        drop(tx);

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

#[test]
fn drop_rx() {
    loom::model(|| {
        let (tx, mut rx1) = broadcast::channel(16);
        let rx2 = tx.subscribe();

        let th1 = thread::spawn(move || {
            block_on(async {
                let v = assert_ok!(rx1.recv().await);
                assert_eq!(v, "one");

                let v = assert_ok!(rx1.recv().await);
                assert_eq!(v, "two");

                let v = assert_ok!(rx1.recv().await);
                assert_eq!(v, "three");

                match assert_err!(rx1.recv().await) {
                    Closed => {}
                    _ => panic!(),
                }
            });
        });

        let th2 = thread::spawn(move || {
            drop(rx2);
        });

        assert_ok!(tx.send("one"));
        assert_ok!(tx.send("two"));
        assert_ok!(tx.send("three"));
        drop(tx);

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

#[test]
fn drop_multiple_rx_with_overflow() {
    loom::model(move || {
        // It is essential to have multiple senders and receivers in this test case.
        let (tx, mut rx) = broadcast::channel(1);
        let _rx2 = tx.subscribe();

        let _ = tx.send(());
        let tx2 = tx.clone();
        let th1 = thread::spawn(move || {
            block_on(async {
                for _ in 0..100 {
                    let _ = tx2.send(());
                }
            });
        });
        let _ = tx.send(());

        let th2 = thread::spawn(move || {
            block_on(async { while let Ok(_) = rx.recv().await {} });
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

// The tests below exercise the sharded waiter list (see #5465). Under loom the
// channel uses a reduced shard count (see `WAITER_SHARDS`), and waiters are
// assigned to shards deterministically, so these models stay small while still
// covering insertion, cross-shard draining, and cancellation races.

// Two receivers park into (possibly different) shards while a single send must
// drain every shard. `Arc` payloads let loom detect leaks / double-frees.
#[test]
fn shard_concurrent_subscribe_send() {
    loom::model(|| {
        let (tx, mut rx1) = broadcast::channel::<Arc<&'static str>>(2);
        let mut rx2 = tx.subscribe();
        let tx = Arc::new(tx);
        let tx2 = tx.clone();

        let th1 = thread::spawn(move || {
            block_on(async {
                assert_eq!(*assert_ok!(rx1.recv().await), "a");
            });
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                assert_eq!(*assert_ok!(rx2.recv().await), "a");
            });
        });

        // Racing the two parking receivers; `notify_rx` must visit every shard.
        assert_ok!(tx2.send(Arc::new("a")));

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

// A receiver parks and must not miss a wakeup even when the buffer wraps (and
// possibly lags) underneath it. With capacity 1 every second send overwrites the
// slot, forcing the receiver onto the lag path.
#[test]
fn shard_wrap_no_lost_wakeup() {
    loom::model(|| {
        let (tx, mut rx) = broadcast::channel(1);
        let tx = Arc::new(tx);
        let tx2 = tx.clone();

        let th = thread::spawn(move || {
            block_on(async {
                let mut num = 0;
                loop {
                    match rx.recv().await {
                        Ok(_) => num += 1,
                        Err(Closed) => break,
                        Err(Lagged(n)) => num += n as usize,
                    }
                }
                // Both values are accounted for as some mix of received + lagged.
                assert_eq!(num, 2);
            });
        });

        assert_ok!(tx2.send(1));
        assert_ok!(tx2.send(2));
        drop(tx2);
        drop(tx);

        assert_ok!(th.join());
    });
}

// Cancelling a parked receiver (dropping its `Recv` future) races the sender's
// `notify_rx` draining the same shard. This exercises `Recv::drop`'s `remove`
// against the drain and the `queued` Acquire/Release handshake; the `Arc`
// payload lets loom detect any double-free / use-after-free.
#[test]
fn shard_cancel_parked_racing_send() {
    loom::model(|| {
        let (tx, mut rx) = broadcast::channel::<Arc<i32>>(2);
        let tx = Arc::new(tx);
        let tx2 = tx.clone();

        let th = thread::spawn(move || {
            let _ = tx2.send(Arc::new(1));
        });

        {
            // Poll once to park (registering a waiter in its shard), then drop
            // the future to cancel it while the send above may be draining.
            let mut fut = task::spawn(rx.recv());
            let _ = fut.poll();
        }

        assert_ok!(th.join());
    });
}

// Two receivers parked into (possibly different) shards are cancelled
// concurrently with a send. Confirms the per-shard locks do not deadlock and
// that concurrent removal + drain stay consistent.
#[test]
fn shard_concurrent_cancels() {
    loom::model(|| {
        let (tx, mut rx1) = broadcast::channel::<Arc<i32>>(2);
        let mut rx2 = tx.subscribe();
        let tx = Arc::new(tx);

        let th = thread::spawn(move || {
            let mut fut = task::spawn(rx2.recv());
            let _ = fut.poll();
        });

        {
            let mut fut = task::spawn(rx1.recv());
            let _ = fut.poll();
        }

        assert_ok!(tx.send(Arc::new(1)));

        assert_ok!(th.join());
    });
}
