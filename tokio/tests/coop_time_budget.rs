#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::time::Duration;
use tokio::runtime::Builder;
use tokio::task::coop::{consume_budget, has_budget_remaining};

/// With time-based coop budgeting, a task should be permitted to consume many
/// more cooperative yield points per poll than the default tick budget (128),
/// provided each yield point is cheap and the wall-clock budget has not yet
/// elapsed.
#[test]
fn time_budget_allows_more_than_tick_budget() {
    let rt = Builder::new_current_thread()
        .enable_all()
        .coop_time_budget(Duration::from_secs(1))
        .build()
        .unwrap();

    rt.block_on(async {
        // The tick-based budget would exhaust after 128 calls. With a 1s
        // time-based budget the loop below should easily exceed that without
        // ever observing exhaustion.
        for _ in 0..1_000 {
            assert!(has_budget_remaining());
            consume_budget().await;
        }
    });
}

/// With time-based coop budgeting, once the configured duration elapses, the
/// budget should be reported as exhausted.
#[test]
fn time_budget_exhausts_after_duration() {
    let rt = Builder::new_current_thread()
        .enable_all()
        .coop_time_budget(Duration::from_millis(10))
        .build()
        .unwrap();

    rt.block_on(async {
        assert!(has_budget_remaining());

        // Burn through the time budget without yielding back to the runtime.
        std::thread::sleep(Duration::from_millis(20));

        assert!(!has_budget_remaining());
    });
}

#[test]
#[should_panic]
fn time_budget_zero_panics() {
    let _ = Builder::new_current_thread().coop_time_budget(Duration::ZERO);
}

#[test]
fn time_budget_works_on_multi_thread() {
    let rt = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .coop_time_budget(Duration::from_secs(1))
        .build()
        .unwrap();

    rt.block_on(async {
        let handle = tokio::spawn(async {
            for _ in 0..1_000 {
                assert!(has_budget_remaining());
                consume_budget().await;
            }
        });
        handle.await.unwrap();
    });
}
