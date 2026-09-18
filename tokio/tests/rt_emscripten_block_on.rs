//! `Runtime::block_on` drives the scheduler synchronously to a fixed point:
//! immediate futures return their value. A wait that would block suspends on
//! the host loop when the build linked `-sJSPI` (see `rt_emscripten_jspi`)
//! and panics when it did not. Both CI lanes run this file.

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

#[test]
fn wait_without_a_deadline_needs_jspi() {
    // A oneshot sent from a host callback: the only wake is an unpark from a
    // later wasm activation, which needs the suspended activation to be
    // resumable.
    let recv = || {
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
        // Without JSPI the receiver is gone by the time this fires.
        host_callback(10, move || {
            let _ = tx.send(11);
        });
        rt().block_on(async { rx.await.unwrap() })
    };

    if jspi_linked() {
        assert_eq!(recv(), 11);
    } else {
        assert_panics_cannot_block_on(|| {
            recv();
        });
    }
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
