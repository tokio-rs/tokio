use crate::sync::watch;

use loom::future::block_on;
use loom::thread;

#[test]
fn smoke() {
    loom::model(|| {
        let (tx, mut rx) = watch::channel(1);

        let th = thread::spawn(move || {
            tx.send(2).unwrap();
        });

        block_on(rx.changed()).unwrap();
        assert_eq!(*rx.borrow(), 2);

        th.join().unwrap();
    })
}
