#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::{self, Runtime};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{self, LocalSet};
use tokio::time;

use std::cell::Cell;
use std::sync::atomic::Ordering::{self, SeqCst};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

#[tokio::test(basic_scheduler)]
async fn local_basic_scheduler() {
    LocalSet::new()
        .run_until(async {
            task::spawn_local(async {}).await.unwrap();
        })
        .await;
}

#[tokio::test(threaded_scheduler)]
async fn local_threadpool() {
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    LocalSet::new()
        .run_until(async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            task::spawn_local(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            })
            .await
            .unwrap();
        })
        .await;
}

#[tokio::test(threaded_scheduler)]
async fn localset_future_threadpool() {
    thread_local! {
        static ON_LOCAL_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_LOCAL_THREAD.with(|cell| cell.set(true));

    let local = LocalSet::new();
    local.spawn_local(async move {
        assert!(ON_LOCAL_THREAD.with(|cell| cell.get()));
    });
    local.await;
}

#[tokio::test(threaded_scheduler)]
async fn localset_future_timers() {
    static RAN1: AtomicBool = AtomicBool::new(false);
    static RAN2: AtomicBool = AtomicBool::new(false);

    let local = LocalSet::new();
    local.spawn_local(async move {
        time::delay_for(Duration::from_millis(10)).await;
        RAN1.store(true, Ordering::SeqCst);
    });
    local.spawn_local(async move {
        time::delay_for(Duration::from_millis(20)).await;
        RAN2.store(true, Ordering::SeqCst);
    });
    local.await;
    assert!(RAN1.load(Ordering::SeqCst));
    assert!(RAN2.load(Ordering::SeqCst));
}

#[tokio::test]
async fn localset_future_drives_all_local_futs() {
    static RAN1: AtomicBool = AtomicBool::new(false);
    static RAN2: AtomicBool = AtomicBool::new(false);
    static RAN3: AtomicBool = AtomicBool::new(false);

    let local = LocalSet::new();
    local.spawn_local(async move {
        task::spawn_local(async {
            task::yield_now().await;
            RAN3.store(true, Ordering::SeqCst);
        });
        task::yield_now().await;
        RAN1.store(true, Ordering::SeqCst);
    });
    local.spawn_local(async move {
        task::yield_now().await;
        RAN2.store(true, Ordering::SeqCst);
    });
    local.await;
    assert!(RAN1.load(Ordering::SeqCst));
    assert!(RAN2.load(Ordering::SeqCst));
    assert!(RAN3.load(Ordering::SeqCst));
}

#[tokio::test(threaded_scheduler)]
async fn local_threadpool_timer() {
    // This test ensures that runtime services like the timer are properly
    // set for the local task set.
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    LocalSet::new()
        .run_until(async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = task::spawn_local(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                time::delay_for(Duration::from_millis(10)).await;
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        })
        .await;
}

#[test]
// This will panic, since the thread that calls `block_on` cannot use
// in-place blocking inside of `block_on`.
#[should_panic]
fn local_threadpool_blocking_in_place() {
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    let mut rt = runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap();
    LocalSet::new().block_on(&mut rt, async {
        assert!(ON_RT_THREAD.with(|cell| cell.get()));
        let join = task::spawn_local(async move {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            task::block_in_place(|| {});
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
        });
        join.await.unwrap();
    });
}

#[tokio::test(threaded_scheduler)]
async fn local_threadpool_blocking_run() {
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    LocalSet::new()
        .run_until(async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = task::spawn_local(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                task::spawn_blocking(|| {
                    assert!(
                        !ON_RT_THREAD.with(|cell| cell.get()),
                        "blocking must not run on the local task set's thread"
                    );
                })
                .await
                .unwrap();
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        })
        .await;
}

#[tokio::test(threaded_scheduler)]
async fn all_spawns_are_local() {
    use futures::future;
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    LocalSet::new()
        .run_until(async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let handles = (0..128)
                .map(|_| {
                    task::spawn_local(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    })
                })
                .collect::<Vec<_>>();
            for joined in future::join_all(handles).await {
                joined.unwrap();
            }
        })
        .await;
}

#[tokio::test(threaded_scheduler)]
async fn nested_spawn_is_local() {
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    LocalSet::new()
        .run_until(async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            task::spawn_local(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                task::spawn_local(async {
                    assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    task::spawn_local(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        task::spawn_local(async {
                            assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        })
                        .await
                        .unwrap();
                    })
                    .await
                    .unwrap();
                })
                .await
                .unwrap();
            })
            .await
            .unwrap();
        })
        .await;
}

#[test]
fn join_local_future_elsewhere() {
    thread_local! {
        static ON_RT_THREAD: Cell<bool> = Cell::new(false);
    }

    ON_RT_THREAD.with(|cell| cell.set(true));

    let mut rt = runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();
    let local = LocalSet::new();
    local.block_on(&mut rt, async move {
        let (tx, rx) = oneshot::channel();
        let join = task::spawn_local(async move {
            println!("hello world running...");
            assert!(
                ON_RT_THREAD.with(|cell| cell.get()),
                "local task must run on local thread, no matter where it is awaited"
            );
            rx.await.unwrap();

            println!("hello world task done");
            "hello world"
        });
        let join2 = task::spawn(async move {
            assert!(
                !ON_RT_THREAD.with(|cell| cell.get()),
                "spawned task should be on a worker"
            );

            tx.send(()).expect("task shouldn't have ended yet");
            println!("waking up hello world...");

            join.await.expect("task should complete successfully");

            println!("hello world task joined");
        });
        join2.await.unwrap()
    });
}

#[test]
fn drop_cancels_tasks() {
    use std::rc::Rc;

    // This test reproduces issue #1842
    let mut rt = rt();
    let rc1 = Rc::new(());
    let rc2 = rc1.clone();

    let (started_tx, started_rx) = oneshot::channel();

    let local = LocalSet::new();
    local.spawn_local(async move {
        // Move this in
        let _rc2 = rc2;

        started_tx.send(()).unwrap();
        loop {
            time::delay_for(Duration::from_secs(3600)).await;
        }
    });

    local.block_on(&mut rt, async {
        started_rx.await.unwrap();
    });
    drop(local);
    drop(rt);

    assert_eq!(1, Rc::strong_count(&rc1));
}

#[test]
fn drop_cancels_remote_tasks() {
    // This test reproduces issue #1885.
    use std::sync::mpsc::RecvTimeoutError;

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let (tx, mut rx) = mpsc::channel::<()>(1024);

        let mut rt = rt();

        let local = LocalSet::new();
        local.spawn_local(async move { while let Some(_) = rx.recv().await {} });
        local.block_on(&mut rt, async {
            time::delay_for(Duration::from_millis(1)).await;
        });

        drop(tx);

        // This enters an infinite loop if the remote notified tasks are not
        // properly cancelled.
        drop(local);

        // Send a message on the channel so that the test thread can
        // determine if we have entered an infinite loop:
        done_tx.send(()).unwrap();
    });

    // Since the failure mode of this test is an infinite loop, rather than
    // something we can easily make assertions about, we'll run it in a
    // thread. When the test thread finishes, it will send a message on a
    // channel to this thread. We'll wait for that message with a fairly
    // generous timeout, and if we don't recieve it, we assume the test
    // thread has hung.
    //
    // Note that it should definitely complete in under a minute, but just
    // in case CI is slow, we'll give it a long timeout.
    match done_rx.recv_timeout(Duration::from_secs(60)) {
        Err(RecvTimeoutError::Timeout) => panic!(
            "test did not complete within 60 seconds, \
             we have (probably) entered an infinite loop!"
        ),
        // Did the test thread panic? We'll find out for sure when we `join`
        // with it.
        Err(RecvTimeoutError::Disconnected) => {
            println!("done_rx dropped, did the test thread panic?");
        }
        // Test completed successfully!
        Ok(()) => {}
    }

    thread.join().expect("test thread should not panic!")
}

#[tokio::test]
async fn local_tasks_are_polled_after_tick() {
    // Reproduces issues #1899 and #1900

    static RX1: AtomicUsize = AtomicUsize::new(0);
    static RX2: AtomicUsize = AtomicUsize::new(0);
    static EXPECTED: usize = 500;

    let (tx, mut rx) = mpsc::unbounded_channel();

    let local = LocalSet::new();

    local
        .run_until(async {
            let task2 = task::spawn(async move {
                // Wait a bit
                time::delay_for(Duration::from_millis(100)).await;

                let mut oneshots = Vec::with_capacity(EXPECTED);

                // Send values
                for _ in 0..EXPECTED {
                    let (oneshot_tx, oneshot_rx) = oneshot::channel();
                    oneshots.push(oneshot_tx);
                    tx.send(oneshot_rx).unwrap();
                }

                time::delay_for(Duration::from_millis(100)).await;

                for tx in oneshots.drain(..) {
                    tx.send(()).unwrap();
                }

                time::delay_for(Duration::from_millis(300)).await;
                let rx1 = RX1.load(SeqCst);
                let rx2 = RX2.load(SeqCst);
                println!("EXPECT = {}; RX1 = {}; RX2 = {}", EXPECTED, rx1, rx2);
                assert_eq!(EXPECTED, rx1);
                assert_eq!(EXPECTED, rx2);
            });

            while let Some(oneshot) = rx.recv().await {
                RX1.fetch_add(1, SeqCst);

                task::spawn_local(async move {
                    oneshot.await.unwrap();
                    RX2.fetch_add(1, SeqCst);
                });
            }

            task2.await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn acquire_mutex_in_drop() {
    use futures::future::pending;

    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();
    let local = LocalSet::new();

    local.spawn_local(async move {
        let _ = rx2.await;
        unreachable!();
    });

    local.spawn_local(async move {
        let _ = rx1.await;
        tx2.send(()).unwrap();
        unreachable!();
    });

    // Spawn a task that will never notify
    local.spawn_local(async move {
        pending::<()>().await;
        tx1.send(()).unwrap();
    });

    // Tick the loop
    local
        .run_until(async {
            task::yield_now().await;
        })
        .await;

    // Drop the LocalSet
    drop(local);
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap()
}
