//! `Runtime::block_on` drives the scheduler synchronously to a fixed point:
//! immediate futures return their value, and a wait with no deadline panics at
//! the park leaf whatever the build links, since nothing could ever wake it. A
//! timed wait suspends on the host loop when the build linked `-sJSPI` (see
//! `rt_emscripten_jspi`) and panics when it did not. Both CI lanes run this
//! file.

#![cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    feature = "rt",
    feature = "time",
    feature = "sync",
    feature = "macros"
))]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use tokio::runtime::Builder;

extern "C" {
    /// Emscripten's `ASYNCIFY` build mode; 2 is JSPI.
    fn emscripten_has_asyncify() -> i32;
}

fn jspi_linked() -> bool {
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    unsafe { emscripten_has_asyncify() == 2 }
}

fn rt() -> tokio::runtime::Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

/// Assert `f` panics with the targeted would-suspend message.
fn assert_panics_cannot_block_on(f: impl FnOnce()) {
    let err = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a would-suspend panic");
    let msg = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("cannot block"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn block_on_returns_immediate_value() {
    let out = rt().block_on(async { 1 + 2 });
    assert_eq!(out, 3);
}

#[test]
fn block_on_drives_ready_spawned_tasks() {
    // Spawned tasks that complete synchronously must be driven to
    // completion within the same fixed-point pump.
    let out = rt().block_on(async {
        let a = tokio::spawn(async { 20 });
        let b = tokio::spawn(async { 22 });
        a.await.unwrap() + b.await.unwrap()
    });
    assert_eq!(out, 42);
}

#[test]
fn timer_wait_needs_jspi() {
    let sleep = || {
        rt().block_on(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
    };

    if jspi_linked() {
        sleep();
    } else {
        assert_panics_cannot_block_on(sleep);
    }
}

// With `net` the wait is a real `epoll_wait`, which a socket could wake.
#[cfg(not(feature = "net"))]
#[test]
fn wait_without_a_deadline_always_panics() {
    // A oneshot whose sender never fires: pending with no wake source, so
    // there is no deadline to suspend on even with JSPI linked.
    assert_panics_cannot_block_on(|| {
        rt().block_on(async {
            let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
            let _ = rx.await;
        });
    });
}
