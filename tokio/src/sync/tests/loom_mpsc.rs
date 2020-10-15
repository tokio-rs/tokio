use crate::sync::mpsc;

use futures::future::poll_fn;
use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;
use tokio_test::assert_ok;

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
