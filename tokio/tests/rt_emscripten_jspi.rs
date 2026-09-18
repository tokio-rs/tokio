//! JSPI suspension contracts. With `-sJSPI` a would-block wait suspends the
//! activation until a host timer fires or a later activation unparks it;
//! without it the wait panics (see `rt_emscripten_block_on`), so every test
//! here returns early unless the build linked JSPI.
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

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Builder;
use tokio::sync::Notify;
use tokio::time::{sleep, Instant};

fn rt() -> tokio::runtime::Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

extern "C" {
    /// Emscripten's `ASYNCIFY` build mode; 2 is JSPI.
    fn emscripten_has_asyncify() -> i32;
}

fn jspi_linked() -> bool {
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    unsafe { emscripten_has_asyncify() == 2 }
}

macro_rules! require_jspi {
    () => {
        if !jspi_linked() {
            return;
        }
    };
}

fn is_nested_runtime_panic(e: &Box<dyn std::any::Any + Send>) -> bool {
    e.downcast_ref::<&str>()
        .map(|m| m.contains("Cannot start a runtime from within a runtime"))
        .unwrap_or(false)
}

#[test]
fn nested_block_on_still_panics() {
    require_jspi!();
    if cfg!(not(panic = "unwind")) {
        return;
    }
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
    require_jspi!();
    let out = rt().block_on(async {
        tokio::task::yield_now().await;
        7
    });
    assert_eq!(out, 7);
}

#[tokio::test]
async fn root_sleep_parks_and_resumes() {
    require_jspi!();
    let start = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        start.elapsed() >= Duration::from_millis(15),
        "the park must actually wait out the timer deadline"
    );
}

#[tokio::test]
async fn root_spawned_tasks_with_timers() {
    require_jspi!();
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
    require_jspi!();
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
    require_jspi!();
    // The spawned task's timer bounds the driver park; on resume it sends
    // and wakes the root future.
    let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.send(11).unwrap();
    });
    assert_eq!(rx.await.unwrap(), 11);
}

extern "C" {
    fn emscripten_async_call(
        func: extern "C" fn(*mut std::ffi::c_void),
        arg: *mut std::ffi::c_void,
        millis: i32,
    );
}

/// Run `f` from a fresh wasm activation after a host timeout.
fn host_callback(millis: i32, f: impl FnOnce() + 'static) {
    extern "C" fn trampoline(arg: *mut std::ffi::c_void) {
        // SAFETY: `arg` is the `Box<Box<dyn FnOnce()>>` leaked below, and
        // Emscripten invokes the callback exactly once.
        let f = unsafe { Box::from_raw(arg as *mut Box<dyn FnOnce()>) };
        f();
    }
    let f: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    // SAFETY: an Emscripten API scheduling `trampoline(arg)` on the host loop.
    unsafe { emscripten_async_call(trampoline, Box::into_raw(f) as *mut _, millis) }
}

// A host callback is a fresh activation entering tokio while the root
// activation is parked with no deadline; its send must resume the park.
#[test]
fn host_activation_wakes_park_without_deadline() {
    require_jspi!();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(1);
    host_callback(10, move || tx.try_send(11).unwrap());
    let out = rt().block_on(async { rx.recv().await.unwrap() });
    assert_eq!(out, 11);
}

// The park is bounded by a far timer; the host callback's send must resume
// it at once rather than at that deadline.
#[test]
fn host_activation_wakes_timed_park_early() {
    require_jspi!();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(1);
    host_callback(10, move || tx.try_send(11).unwrap());
    let start = Instant::now();
    let out = rt().block_on(async {
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
    });
    assert_eq!(out, 11);
    assert!(start.elapsed() < Duration::from_secs(5));
}

// A spawned task woken from a host activation, with the root awaiting it.
#[test]
fn host_activation_wakes_spawned_task() {
    require_jspi!();
    let notify = Arc::new(Notify::new());
    let n = notify.clone();
    host_callback(10, move || n.notify_one());
    let out = rt().block_on(async {
        tokio::spawn(async move {
            notify.notified().await;
            5
        })
        .await
        .unwrap()
    });
    assert_eq!(out, 5);
}

// A self-rewaking task must not starve the real host timer: the
// event-interval park yields a 0ms host turn so the timer still fires.
#[tokio::test]
async fn greedy_task_does_not_starve_host_timer() {
    require_jspi!();
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
    require_jspi!();
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
