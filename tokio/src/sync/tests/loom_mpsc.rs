use crate::sync::mpsc;

use futures::future::poll_fn;
use loom::future::block_on;
use loom::thread;

#[test]
fn closing_tx() {
    loom::model(|| {
        let (mut tx, mut rx) = mpsc::channel(16);

        thread::spawn(move || {
            tx.try_send(()).unwrap();
            drop(tx);
        });

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
        assert!(v.is_some());

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
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

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
        assert!(v.is_some());

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
        assert!(v.is_none());
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

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
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

        let v = block_on(poll_fn(|cx| rx.poll_recv(cx)));
        assert!(v.is_none());
    });
}
