//! `Runtime::block_on` outside a JSPI promising activation drives the
//! scheduler synchronously to a fixed point: immediate futures return their
//! value; any wait panics at the park leaf, in both CI lanes (JSPI linked
//! and not), since only the `#[tokio::test]` activation can suspend (see
//! `rt_emscripten_jspi`).

#![cfg(all(
    target_os = "emscripten",
    feature = "rt",
    feature = "time",
    feature = "macros"
))]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use tokio::runtime::Builder;

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
fn timer_wait_panics_outside_activation() {
    assert_panics_cannot_block_on(|| {
        rt().block_on(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
    });
}

#[test]
fn external_wait_panics_outside_activation() {
    // A oneshot whose sender never fires: pending with no wake source.
    assert_panics_cannot_block_on(|| {
        rt().block_on(async {
            let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
            let _ = rx.await;
        });
    });
}
