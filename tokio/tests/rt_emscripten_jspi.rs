//! JSPI suspension contracts. With `-sJSPI` a would-block wait suspends on a
//! host timer while the host loop delivers wakes; without it the wait panics
//! (see `rt_emscripten_block_on`). Only the JSPI CI lane runs this file.
//!
//! NOTE: This is the only Emscripten test file with real timer tests.

#![cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    feature = "rt",
    feature = "time",
    feature = "sync",
    feature = "macros"
))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Builder;
use tokio::sync::Notify;
use tokio::time::{sleep, Instant};

fn rt() -> tokio::runtime::Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

// From `rt_emscripten_jspi.js`, which calls the `tokio_test_*` exports below.
extern "C" {
    fn tokio_test_schedule_reenter(ms: f64);
    fn tokio_test_await_reenter() -> i32;
    fn tokio_test_reenter_sync_call() -> i32;
    fn tokio_test_call_unsuspendable() -> i32;
}

// Non-promising export whose park cannot suspend; the JS error unwinds out.
#[no_mangle]
pub extern "C-unwind" fn tokio_test_unsuspendable() {
    rt().block_on(async { sleep(Duration::from_millis(5)).await });
}

// Promising export: a sibling runtime that parks while the caller is parked.
#[no_mangle]
pub extern "C" fn tokio_test_reenter() -> i32 {
    rt().block_on(async {
        let task = tokio::spawn(async {
            sleep(Duration::from_millis(5)).await;
            40
        });
        task.await.unwrap() + 2
    })
}

// Non-promising export called during a task-issued suspension, where the
// runtime is still entered.
#[no_mangle]
pub extern "C" fn tokio_test_reenter_sync() -> i32 {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(|| rt().block_on(async { 1 }));
    std::panic::set_hook(hook);
    match res {
        Ok(v) => v,
        Err(e) if is_nested_runtime_panic(&e) => -1,
        Err(_) => -2,
    }
}

fn is_nested_runtime_panic(e: &Box<dyn std::any::Any + Send>) -> bool {
    e.downcast_ref::<&str>()
        .map(|m| m.contains("Cannot start a runtime from within a runtime"))
        .unwrap_or(false)
}

// Runtime B enters, parks and completes while runtime A is parked. A must
// stay parked throughout: Emscripten shares one shadow stack between
// promising activations, so A running would overwrite B's frames.
#[test]
fn sibling_block_on_during_park() {
    let start = Instant::now();
    let rt = rt();
    unsafe { tokio_test_schedule_reenter(5.0) };
    let ran = rt.block_on(async {
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(40)).await;
            r.store(true, Ordering::SeqCst);
        });
        sleep(Duration::from_millis(60)).await;
        // B's runtime is gone; this spawn only works if we are back on A's.
        tokio::spawn(async { 7 }).await.unwrap();
        ran.load(Ordering::SeqCst)
    });
    assert!(ran);
    assert!(start.elapsed() >= Duration::from_millis(60));
    assert_eq!(unsafe { tokio_test_await_reenter() }, 42);
}

// A runs while B is suspended, then B resumes.
#[test]
#[ignore = "needs Emscripten shadow stack switching between promising activations"]
fn interleaved_suspended_runtimes() {
    let rt = rt();
    unsafe { tokio_test_schedule_reenter(5.0) };
    let sum = rt.block_on(async {
        // B enters at 5ms and parks for 5ms; A wakes at 7ms.
        sleep(Duration::from_millis(7)).await;
        let a = tokio::spawn(async {
            tokio::task::yield_now().await;
            1
        });
        let b = tokio::spawn(async {
            sleep(Duration::from_millis(2)).await;
            2
        });
        let sum = a.await.unwrap() + b.await.unwrap();
        sleep(Duration::from_millis(20)).await;
        sum
    });
    assert_eq!(sum, 3);
    assert_eq!(unsafe { tokio_test_await_reenter() }, 42);
}

// A suspension issued from task code is not a park; a sibling `block_on`
// during it is nested.
#[tokio::test]
async fn task_suspension_is_not_a_leave() {
    let code = tokio::spawn(async { unsafe { tokio_test_reenter_sync_call() } })
        .await
        .unwrap();
    assert_eq!(code, -1);
}

// A park with no suspender throws `SuspendError` out of the import. The
// unwind must restore the context so the thread is usable afterwards.
#[test]
fn unsuspendable_park_unwinds_cleanly() {
    assert_eq!(unsafe { tokio_test_call_unsuspendable() }, 1);
    assert_eq!(rt().block_on(async { 1 }), 1);
}

#[test]
fn nested_block_on_still_panics() {
    let outer = rt();
    let res = outer.block_on(async {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let res = std::panic::catch_unwind(|| rt().block_on(async { 1 }));
        std::panic::set_hook(hook);
        res
    });
    assert!(is_nested_runtime_panic(&res.unwrap_err()));
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
