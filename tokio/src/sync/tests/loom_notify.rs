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
                rx.notified().await;
            });
        });

        tx.notify_one();
        th.join().unwrap();
    });
}

#[test]
fn notify_waiters() {
    loom::model(|| {
        let notify = Arc::new(Notify::new());
        let tx = notify.clone();
        let notified1 = notify.notified();
        let notified2 = notify.notified();

        let th = thread::spawn(move || {
            tx.notify_waiters();
        });

        block_on(async {
            notified1.await;
            notified2.await;
        });

        th.join().unwrap();
    });
}

fn notify_waiters_poll_consistency_variant(poll_setting: [bool; 2]) {
    use tokio_test::assert_pending;

    let notify = Arc::new(Notify::new());
    let mut notified = [
        tokio_test::task::spawn(notify.notified()),
        tokio_test::task::spawn(notify.notified()),
    ];
    for i in 0..2 {
        if poll_setting[i] {
            assert_pending!(notified[i].poll());
        }
    }

    let tx = notify.clone();
    let th = thread::spawn(move || {
        tx.notify_waiters();
    });

    let res1 = notified[0].poll();
    let res2 = notified[1].poll();

    // If res1 is ready, then res2 must also be ready.
    assert!(res1.is_pending() || res2.is_ready());

    th.join().unwrap();
}

#[test]
fn notify_waiters_poll_consistency() {
    // We test different scenarios where pending futures had or had not
    // been polled before the call to `notify_waiters`.
    loom::model(|| notify_waiters_poll_consistency_variant([false, false]));
    loom::model(|| notify_waiters_poll_consistency_variant([true, false]));
    loom::model(|| notify_waiters_poll_consistency_variant([false, true]));
    loom::model(|| notify_waiters_poll_consistency_variant([true, true]));
}

#[test]
fn notify_waiters_and_one() {
    loom::model(|| {
        let notify = Arc::new(Notify::new());
        let tx1 = notify.clone();
        let tx2 = notify.clone();

        let th1 = thread::spawn(move || {
            tx1.notify_waiters();
        });

        let th2 = thread::spawn(move || {
            tx2.notify_one();
        });

        let th3 = thread::spawn(move || {
            let notified = notify.notified();

            block_on(async {
                notified.await;
            });
        });

        th1.join().unwrap();
        th2.join().unwrap();
        th3.join().unwrap();
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
                    notify.notified().await;
                    notify.notify_one();
                })
            }));
        }

        notify.notify_one();

        for th in ths.drain(..) {
            th.join().unwrap();
        }

        block_on(async {
            notify.notified().await;
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
            let mut recv = Box::pin(rx1.notified());

            block_on(poll_fn(|cx| {
                if recv.as_mut().poll(cx).is_ready() {
                    rx1.notify_one();
                }
                Poll::Ready(())
            }));
        });

        let th2 = thread::spawn(move || {
            block_on(async {
                rx2.notified().await;
                // Trigger second notification
                rx2.notify_one();
                rx2.notified().await;
            });
        });

        notify.notify_one();

        th1.join().unwrap();
        th2.join().unwrap();
    });
}

#[test]
fn notify_waiters_sequential() {
    use crate::sync::oneshot;
    use tokio_test::assert_pending;

    loom::model(|| {
        let notify = Arc::new(Notify::new());

        let (tx_fst, rx_fst) = oneshot::channel();
        let (tx_snd, rx_snd) = oneshot::channel();

        let receiver = thread::spawn({
            let notify = notify.clone();
            move || {
                block_on(async {
                    // Poll the first `Notified` to put it as the first waiter
                    // in the queue.
                    let mut first_notified = tokio_test::task::spawn(notify.notified());
                    assert_pending!(first_notified.poll());

                    // Create additional waiters to force `notify_waiters` to
                    // release the lock at least once.
                    const WAKE_LIST_SIZE: usize = 32;
                    let _task_pile = (0..WAKE_LIST_SIZE + 1)
                        .map(|_| {
                            let mut fut = tokio_test::task::spawn(notify.notified());
                            assert_pending!(fut.poll());
                            fut
                        })
                        .collect::<Vec<_>>();

                    // We are ready for the notify_waiters call.
                    tx_fst.send(()).unwrap();

                    first_notified.await;

                    // Poll the second `Notified` future to try to insert
                    // it to the waiters queue.
                    let mut second_notified = tokio_test::task::spawn(notify.notified());
                    assert_pending!(second_notified.poll());

                    // Wait for the `notify_waiters` to end and check if we
                    // are woken up.
                    rx_snd.await.unwrap();
                    assert_pending!(second_notified.poll());
                });
            }
        });

        // Wait for the signal and call `notify_waiters`.
        block_on(rx_fst).unwrap();
        notify.notify_waiters();
        tx_snd.send(()).unwrap();

        receiver.join().unwrap();
    });
}
