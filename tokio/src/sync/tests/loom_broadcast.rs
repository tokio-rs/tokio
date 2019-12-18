use crate::sync::broadcast;
use crate::sync::broadcast::RecvError::{Closed, Lagged};

use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;
use tokio_test::{assert_err, assert_ok};

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
                        Err(Lagged(n)) => {
                            num += n as usize
                        },
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
                        Err(Lagged(n)) => {
                            num += n as usize
                        }
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
