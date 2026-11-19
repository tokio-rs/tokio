use crate::sync::mpsc::{self, BLOCK_CAP};

use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;
use std::future::poll_fn;
use tokio_test::assert_ok;

#[test]
fn drop_rx_during_permit_send() {
    drop_rx_during_permit_send_at(0);
}

#[test]
fn drop_rx_during_permit_send_at_block_boundary() {
    drop_rx_during_permit_send_at(BLOCK_CAP);
}

fn drop_rx_during_permit_send_at(start: usize) {
    loom::model(move || {
        let (tx, mut rx) = mpsc::channel(BLOCK_CAP + 2);
        // Exercise both an empty block and growth after a consumed block.
        for _ in 0..start {
            let (reply, _) = crate::sync::oneshot::channel::<()>();
            tx.try_send(reply).unwrap();
            drop(rx.try_recv().unwrap());
        }
        let weak = tx.downgrade();
        let permit1 = tx.clone().try_reserve_owned().unwrap();
        let permit2 = tx.clone().try_reserve_owned().unwrap();
        let (reply1, mut response1) = crate::sync::oneshot::channel::<()>();
        let (reply2, mut response2) = crate::sync::oneshot::channel::<()>();

        let send1 = thread::spawn(move || permit1.send(reply1));
        let send2 = thread::spawn(move || permit2.send(reply2));
        drop(rx);
        let tx1 = send1.join().unwrap();
        let tx2 = send2.join().unwrap();

        assert_eq!(
            response1.try_recv(),
            Err(crate::sync::oneshot::error::TryRecvError::Closed)
        );
        assert_eq!(
            response2.try_recv(),
            Err(crate::sync::oneshot::error::TryRecvError::Closed)
        );
        assert_eq!(tx.capacity(), BLOCK_CAP + 2);
        drop((tx1, tx2, tx, weak));
    });
}

#[test]
fn drop_rx_with_ready_prefix() {
    loom::model(|| {
        let (tx, rx) = mpsc::channel(BLOCK_CAP + 2);
        let value = Arc::new(());
        for _ in 0..BLOCK_CAP + 1 {
            tx.try_send(value.clone()).unwrap();
        }

        let permit = tx.clone().try_reserve_owned().unwrap();
        let sent = value.clone();
        let send = thread::spawn(move || permit.send(sent));
        drop(rx);
        let _sender = send.join().unwrap();

        assert_eq!(Arc::strong_count(&value), 1);
        assert_eq!(tx.capacity(), BLOCK_CAP + 2);
    });
}

#[test]
fn drop_rx_during_bounded_send() {
    loom::model(|| {
        let (tx, rx) = mpsc::channel(1);
        let sender = tx.clone();
        let (reply, mut response) = crate::sync::oneshot::channel::<()>();
        let send = thread::spawn(move || drop(block_on(sender.send(reply))));
        drop(rx);
        send.join().unwrap();
        assert_eq!(
            response.try_recv(),
            Err(crate::sync::oneshot::error::TryRecvError::Closed)
        );
        assert_eq!(tx.capacity(), 1);
        drop(tx);
    });
}

#[test]
fn drop_rx_during_unbounded_send() {
    drop_rx_during_unbounded_send_at(0);
}

#[test]
fn drop_rx_during_unbounded_send_at_block_boundary() {
    drop_rx_during_unbounded_send_at(BLOCK_CAP);
}

fn drop_rx_during_unbounded_send_at(start: usize) {
    loom::model(move || {
        let (tx, mut rx) = mpsc::unbounded_channel();
        for _ in 0..start {
            let (reply, _) = crate::sync::oneshot::channel::<()>();
            tx.send(reply).unwrap();
            drop(rx.try_recv().unwrap());
        }
        let sender1 = tx.clone();
        let sender2 = tx.clone();
        let (reply1, mut response1) = crate::sync::oneshot::channel::<()>();
        let (reply2, mut response2) = crate::sync::oneshot::channel::<()>();

        // Drop SendError too: either the send fails or the channel drops the value.
        let send1 = thread::spawn(move || drop(sender1.send(reply1)));
        let send2 = thread::spawn(move || drop(sender2.send(reply2)));
        drop(rx);
        send1.join().unwrap();
        send2.join().unwrap();

        assert_eq!(
            response1.try_recv(),
            Err(crate::sync::oneshot::error::TryRecvError::Closed)
        );
        assert_eq!(
            response2.try_recv(),
            Err(crate::sync::oneshot::error::TryRecvError::Closed)
        );
        drop(tx);
    });
}

#[test]
fn close_rx_during_permit_send() {
    loom::model(|| {
        let (tx, mut rx) = mpsc::channel(1);
        let permit = tx.try_reserve_owned().unwrap();
        let send = thread::spawn(move || permit.send(123));
        rx.close();
        let tx = send.join().unwrap();
        assert_eq!(rx.try_recv(), Ok(123));
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Disconnected));
        drop(tx);
    });
}

#[test]
fn closing_tx() {
    loom::model(|| {
        let (tx, mut rx) = mpsc::channel(16);

        thread::spawn(move || {
            tx.try_send(()).unwrap();
            drop(tx);
        });

        let v = block_on(rx.recv());
        assert!(v.is_some());

        let v = block_on(rx.recv());
        assert!(v.is_none());
    });
}

#[test]
fn closing_unbounded_tx() {
    loom::model(|| {
        let (tx, mut rx) = mpsc::unbounded_channel();

        thread::spawn(move || {
            tx.send(()).unwrap();
            drop(tx);
        });

        let v = block_on(rx.recv());
        assert!(v.is_some());

        let v = block_on(rx.recv());
        assert!(v.is_none());
    });
}

#[test]
fn closing_bounded_rx() {
    loom::model(|| {
        let (tx1, rx) = mpsc::channel::<()>(16);
        let tx2 = tx1.clone();
        thread::spawn(move || {
            drop(rx);
        });

        block_on(tx1.closed());
        block_on(tx2.closed());
    });
}

#[test]
fn closing_and_sending() {
    loom::model(|| {
        let (tx1, mut rx) = mpsc::channel::<()>(16);
        let tx1 = Arc::new(tx1);
        let tx2 = tx1.clone();

        let th1 = thread::spawn(move || {
            tx1.try_send(()).unwrap();
        });

        let th2 = thread::spawn(move || {
            block_on(tx2.closed());
        });

        let th3 = thread::spawn(move || {
            let v = block_on(rx.recv());
            assert!(v.is_some());
            drop(rx);
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

#[test]
fn closing_unbounded_rx() {
    loom::model(|| {
        let (tx1, rx) = mpsc::unbounded_channel::<()>();
        let tx2 = tx1.clone();
        thread::spawn(move || {
            drop(rx);
        });

        block_on(tx1.closed());
        block_on(tx2.closed());
    });
}

#[test]
fn dropping_tx() {
    loom::model(|| {
        let (tx, mut rx) = mpsc::channel::<()>(16);

        for _ in 0..2 {
            let tx = tx.clone();
            thread::spawn(move || {
                drop(tx);
            });
        }
        drop(tx);

        let v = block_on(rx.recv());
        assert!(v.is_none());
    });
}

#[test]
fn dropping_unbounded_tx() {
    loom::model(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<()>();

        for _ in 0..2 {
            let tx = tx.clone();
            thread::spawn(move || {
                drop(tx);
            });
        }
        drop(tx);

        let v = block_on(rx.recv());
        assert!(v.is_none());
    });
}

#[test]
fn try_recv() {
    loom::model(|| {
        use crate::sync::{mpsc, Semaphore};
        use loom::sync::{Arc, Mutex};

        const PERMITS: usize = 2;
        const TASKS: usize = 2;
        const CYCLES: usize = 1;

        struct Context {
            sem: Arc<Semaphore>,
            tx: mpsc::Sender<()>,
            rx: Mutex<mpsc::Receiver<()>>,
        }

        fn run(ctx: &Context) {
            block_on(async {
                let permit = ctx.sem.acquire().await;
                assert_ok!(ctx.rx.lock().unwrap().try_recv());
                crate::task::yield_now().await;
                assert_ok!(ctx.tx.clone().try_send(()));
                drop(permit);
            });
        }

        let (tx, rx) = mpsc::channel(PERMITS);
        let sem = Arc::new(Semaphore::new(PERMITS));
        let ctx = Arc::new(Context {
            sem,
            tx,
            rx: Mutex::new(rx),
        });

        for _ in 0..PERMITS {
            assert_ok!(ctx.tx.clone().try_send(()));
        }

        let mut threads = Vec::new();

        for _ in 0..TASKS {
            let ctx = ctx.clone();

            threads.push(thread::spawn(move || {
                run(&ctx);
            }));
        }

        run(&ctx);

        for thread in threads {
            thread.join().unwrap();
        }
    });
}

#[test]
fn len_nonzero_after_send() {
    loom::model(|| {
        let (send, recv) = mpsc::channel(10);
        let send2 = send.clone();

        let join = thread::spawn(move || {
            block_on(send2.send("message2")).unwrap();
        });

        block_on(send.send("message1")).unwrap();
        assert!(recv.len() != 0);

        join.join().unwrap();
    });
}

#[test]
fn nonempty_after_send() {
    loom::model(|| {
        let (send, recv) = mpsc::channel(10);
        let send2 = send.clone();

        let join = thread::spawn(move || {
            block_on(send2.send("message2")).unwrap();
        });

        block_on(send.send("message1")).unwrap();
        assert!(!recv.is_empty());

        join.join().unwrap();
    });
}

#[test]
fn is_empty_during_close() {
    loom::model(|| {
        let (tx, rx) = mpsc::channel::<()>(1);

        let th1 = thread::spawn(move || {
            assert!(rx.is_empty());
        });

        drop(tx);

        th1.join().unwrap();
    });
}

fn len_during_close_helper(n: usize) {
    loom::model(move || {
        let (tx, rx) = mpsc::channel::<()>(n + 1);

        for _ in 0..n {
            tx.try_send(()).unwrap();
        }

        let th1 = thread::spawn(move || {
            assert_eq!(rx.len(), n);
        });

        drop(tx);

        th1.join().unwrap();
    });
}

#[test]
fn len_during_close_0() {
    len_during_close_helper(0);
}

#[test]
fn len_during_close_1() {
    len_during_close_helper(1);
}

#[test]
fn len_during_close_block_cap() {
    len_during_close_helper(BLOCK_CAP);
}

#[test]
fn len_during_close_block_cap_plus_1() {
    len_during_close_helper(BLOCK_CAP + 1);
}
