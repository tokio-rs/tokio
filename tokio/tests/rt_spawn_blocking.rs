#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), not(miri)))]

use std::time::Duration;

use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

#[test]
fn issue_8056_regression_test() {
    type Senders = Vec<mpsc::Sender<()>>;
    type Handles = JoinSet<()>;

    fn make_writer() -> (Senders, Handles) {
        let mut senders = vec![];
        let mut handles = JoinSet::new();
        for _ in 0..2 {
            let (tx, mut rx) = mpsc::channel::<()>(1);
            senders.push(tx);
            handles.spawn_blocking(move || while rx.blocking_recv().is_some() {});
        }
        (senders, handles)
    }

    async fn drive_writer(senders: Senders, mut handles: Handles) {
        for tx in &senders {
            tx.send(()).await.unwrap();
        }
        drop(senders);
        while let Some(res) = handles.join_next().await {
            res.unwrap();
        }
    }

    // Regression test for the lost-spawn race in the blocking pool introduced
    // by #7757. See https://github.com/tokio-rs/tokio/issues/8056 for complete
    // details.
    //
    // The bug was a race condition in `Spawner::spawn_task` - after pushing a
    // task to the queue, it chose between "wake an idle worker" and "spawn a
    // new worker", by reading the `num_idle_threads` `AtomicUsize`. The
    // `num_idle_threads` counter was incremented by a worker before it calls
    // `wait_for_task`, and was deremented after `Inner::run` after
    // `wait_for_task` had already returned a `Task`.
    //
    // In that window, the counter indicates a worker is idle even though it
    // has already claimed a notification and is about to run a task. In the
    // the omnicron test case that reproduced this, this task was long lived,
    // which would essential result in "under spawning" workers.
    //
    // This test attempts to reproduce that scenario by doing the following on
    // a fresh runtime
    // - Two awaited `spawn_blocking` calls to get pool workers cycling through
    //   idle → notified → busy transitions.
    // - `make_writer` spawns two closures that park in
    //   `mpsc::Receiver::blocking_recv` — long-lived blocking tasks that turn
    //    a stranded spawn into a real deadlock instead of a transient stall.
    // - Two more rounds of "create a writer and immediately drop it" churn the
    //   worker pool while those persistent tasks are still blocked, and then a
    //   second persistent writer is created — this `spawn_blocking` is most
    //   likely to trigger the bug, because the prior churn has left a worker
    //   mid-transition with a stale `num_idle_threads`.
    // - Finally, we finish all the tasks -- if any of them never got pulled
    //   from the `spawn_blocking` queue then it hits the timeout.

    // Run multiple times to make hitting the race condition very likely.
    for _ in 0..512 {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        let completed = rt.block_on(async {
            // We use a timeout to turn a hang into a reliable (and fast)
            // failure. On working code this needs like a millisecond, so any
            // timeout will do.
            tokio::time::timeout(Duration::from_secs(1), async {
                let quick = || async {
                    tokio::task::spawn_blocking(|| {}).await.unwrap();
                };

                quick().await;
                quick().await;
                let (persistent_senders, persistent_handles) = make_writer();

                for _ in 0..2 {
                    quick().await;
                    quick().await;
                    let (senders, mut handles) = make_writer();
                    drop(senders);
                    while let Some(res) = handles.join_next().await {
                        res.unwrap();
                    }
                }

                quick().await;
                quick().await;
                let (second_senders, second_handles) = make_writer();

                drive_writer(persistent_senders, persistent_handles).await;
                drive_writer(second_senders, second_handles).await;
            })
            .await
            .is_ok()
        });

        assert!(completed);
    }
}
