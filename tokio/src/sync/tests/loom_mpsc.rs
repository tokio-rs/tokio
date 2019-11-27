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
