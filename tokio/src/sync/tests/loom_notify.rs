use crate::sync::Notify;

use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;

#[test]
fn notify_one() {
    loom::model(|| {
        let tx = Arc::new(Notify::new());
        let rx = tx.clone();

        let th = thread::spawn(move || {
            block_on(async {
                rx.recv().await;
            });
        });

        tx.notify_one();
        th.join().unwrap();
    });
}

#[test]
fn notify_multi() {
    loom::model(|| {
        let notify = Arc::new(Notify::new());

        let mut ths = vec![];

        for _ in 0..2 {
            let notify = notify.clone();

            ths.push(thread::spawn(move || {
                block_on(async {
                    notify.recv().await;
                    notify.notify_one();
                })
            }));
        }

        notify.notify_one();

        for th in ths.drain(..) {
            th.join().unwrap();
        }

        block_on(async {
            notify.recv().await;
        });
    });
}

#[test]
fn notify_drop() {
    use crate::future::poll_fn;
    use std::future::Future;
    use std::task::Poll;

    loom::model(|| {
        let notify = Arc::new(Notify::new());
        let rx1 = notify.clone();
        let rx2 = notify.clone();

        let th1 = thread::spawn(move || {
            let mut recv = Box::pin(async {
                rx1.recv().await;
                rx1.notify_one();
            });

            block_on(poll_fn(|cx| {
                let _ = recv.as_mut().poll(cx);
                Poll::Ready(())
            }));
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                rx2.recv().await;
            });
        });

        notify.notify_one();

        th1.join().unwrap();
        th2.join().unwrap();
    });
}
