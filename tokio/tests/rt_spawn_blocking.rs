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

    // The bug was a race condition in task spawning when deciding whether to
    // spawn a new blocking worker or to wake an idle one.

    // Run multiple times to make hitting the race condition very likely.
    for _ in 0..512 {
        // Fresh runtime per iteration, matching `#[tokio::test]`
        // semantics in omicron. On the buggy code, a freshly-created
        // blocking pool growing its first few workers is apparently
        // where the stale-read race fires most reliably.
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        let completed = rt.block_on(async {
            // We use a timeout to turn a hang into a reliable (and fast)
            // failure. On working code this needs like a millisecond, so any
            // timeout will do.
            tokio::time::timeout(Duration::from_secs(1), async {
                // Test case is derived from the failing test in omicron:
                // We start with a few very quick `spawn_blocking` operations.
                // Those awaits return the async worker to the pool briefly,
                // letting blocking-pool workers transition between the idle
                // and running states.
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
