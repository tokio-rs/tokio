//! JSPI suspension contracts. With `-sJSPI` a would-block wait in a
//! `#[tokio::test]` body suspends on a host timer while the host loop
//! delivers wakes. Without it a wait throws (see `rt_emscripten_block_on`).
//!
//! NOTE: This is the only Emscripten test file with real timer tests.

#![cfg(all(
    target_os = "emscripten",
    feature = "rt",
    feature = "time",
    feature = "sync",
    feature = "macros"
))]

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Builder;
use tokio::sync::Notify;
use tokio::time::{sleep, Instant};

fn rt() -> tokio::runtime::Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

#[test]
fn block_on_yield_now_takes_a_host_turn() {
    let out = rt().block_on(async {
        tokio::task::yield_now().await;
        7
    });
    assert_eq!(out, 7);
}

#[tokio::test]
async fn root_sleep_parks_and_resumes() {
    let start = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        start.elapsed() >= Duration::from_millis(15),
        "the park must actually wait out the timer deadline"
    );
}

#[tokio::test]
async fn root_spawned_tasks_with_timers() {
    let out = async {
        let a = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            20
        });
        let b = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            22
        });
        a.await.unwrap() + b.await.unwrap()
    }
    .await;
    assert_eq!(out, 42);
}

#[tokio::test]
async fn sequential_parks_inside_one_root() {
    // Each park must suspend and resume independently; leaf bookkeeping
    // must balance across them.
    for i in 0..3u32 {
        let start = tokio::time::Instant::now();
        tokio::time::sleep(Duration::from_millis(2)).await;
        assert!(start.elapsed() >= Duration::from_millis(1), "park {i}");
    }
}

#[tokio::test]
async fn root_park_resumes_on_timer_driven_wake() {
    // The spawned task's timer bounds the driver park; on resume it sends
    // and wakes the root future.
    let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.send(11).unwrap();
    });
    assert_eq!(rx.await.unwrap(), 11);
}

// A self-rewaking task must not starve the real host timer: the
// event-interval park yields a 0ms host turn so the timer still fires.
#[tokio::test]
async fn greedy_task_does_not_starve_host_timer() {
    tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    sleep(Duration::from_millis(5)).await;
}

// When a nearer timer fires, the next park must re-arm for a still-pending
// farther timer rather than dropping it.
#[tokio::test]
async fn farther_timer_survives_nearer_timer_firing() {
    let start = Instant::now();

    let notify = Arc::new(Notify::new());
    let n = notify.clone();
    let near = tokio::spawn(async move {
        sleep(Duration::from_millis(5)).await;
        n.notify_one();
    });
    let waiter = tokio::spawn(async move {
        notify.notified().await;
    });

    sleep(Duration::from_millis(25)).await;
    assert!(
        start.elapsed() >= Duration::from_millis(25),
        "farther timer did not hold its deadline"
    );

    near.await.unwrap();
    waiter.await.unwrap();
}
