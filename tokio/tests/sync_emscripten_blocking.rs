//! `blocking_recv` outside a runtime parks the thread parker directly, which
//! suspends under `-sJSPI` just as the runtime driver does, and panics
//! without it. Needs only the `sync` feature, so every Emscripten CI lane
//! runs this file, including a `sync`-only build.

#![cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    feature = "sync"
))]

use std::panic::{catch_unwind, AssertUnwindSafe};

extern "C" {
    /// Emscripten's `ASYNCIFY` build mode; 2 is JSPI.
    fn emscripten_has_asyncify() -> i32;

    fn emscripten_async_call(
        func: extern "C" fn(*mut std::ffi::c_void),
        arg: *mut std::ffi::c_void,
        millis: i32,
    );
}

fn jspi_linked() -> bool {
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    unsafe { emscripten_has_asyncify() == 2 }
}

/// Run `f` from a fresh wasm activation on the next host loop turn.
fn host_callback(f: impl FnOnce() + 'static) {
    extern "C" fn trampoline(arg: *mut std::ffi::c_void) {
        // SAFETY: `arg` is the `Box<Box<dyn FnOnce()>>` leaked below, and
        // Emscripten invokes the callback exactly once.
        let f = unsafe { Box::from_raw(arg as *mut Box<dyn FnOnce()>) };
        f();
    }
    let f: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    // SAFETY: an Emscripten API scheduling `trampoline(arg)` on the host loop.
    unsafe { emscripten_async_call(trampoline, Box::into_raw(f) as *mut _, 0) }
}

/// Assert `f` panics with the targeted would-suspend message.
fn assert_panics_cannot_block_on(f: impl FnOnce()) {
    if cfg!(not(panic = "unwind")) {
        return;
    }
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
fn mpsc_blocking_recv_wakes_from_host_activation() {
    let recv = || {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(1);
        // Without JSPI the receiver is gone by the time this fires.
        host_callback(move || {
            let _ = tx.try_send(11);
        });
        rx.blocking_recv()
    };

    if jspi_linked() {
        assert_eq!(recv(), Some(11));
    } else {
        assert_panics_cannot_block_on(|| {
            recv();
        });
    }
}

#[test]
fn oneshot_blocking_recv_wakes_from_host_activation() {
    let recv = || {
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
        host_callback(move || {
            let _ = tx.send(11);
        });
        rx.blocking_recv()
    };

    if jspi_linked() {
        assert_eq!(recv(), Ok(11));
    } else {
        assert_panics_cannot_block_on(|| {
            let _ = recv();
        });
    }
}

#[test]
fn blocking_recv_ready_value_needs_no_suspension() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(1);
    tx.try_send(3).unwrap();
    assert_eq!(rx.blocking_recv(), Some(3));
}

#[test]
fn sequential_blocking_recvs_reuse_the_parker() {
    if !jspi_linked() {
        return;
    }
    // Each call parks and resumes independently; the parker's notification
    // token must not leak from one into the next.
    for i in 0..3u32 {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(1);
        host_callback(move || tx.try_send(i).unwrap());
        assert_eq!(rx.blocking_recv(), Some(i));
    }
}
