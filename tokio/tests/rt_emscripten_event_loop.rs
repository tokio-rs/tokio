//! `EventLoopRuntime` contracts that hold on the caller's stack: `schedule`
//! queues only, `drive` runs one batch, wakes from host context never run
//! tasks inline, and `block_on` is rejected. Plain `#[test]`s, so the bodies
//! run outside any drive. What needs the host loop itself (timer and
//! readiness re-drives after `main` returns) is `rt_emscripten_event_loop_main`.

#![cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable,
    feature = "rt",
    feature = "time",
    feature = "sync"
))]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use tokio::runtime::{Builder, EventLoopRuntime};

fn event_loop_rt() -> EventLoopRuntime {
    Builder::new_current_thread()
        .enable_all()
        .build_event_loop_runtime()
        .unwrap()
}

fn panic_message(err: &Box<dyn std::any::Any + Send>) -> &str {
    err.downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("")
}

#[test]
fn schedule_queues_and_drive_runs() {
    let rt = event_loop_rt();
    let ran = Rc::new(Cell::new(false));
    let done = Rc::new(Cell::new(false));

    let ran2 = ran.clone();
    let done2 = done.clone();
    rt.schedule(
        async move {
            ran2.set(true);
            3
        },
        move |out| {
            assert_eq!(out.unwrap(), 3);
            done2.set(true);
        },
    );
    assert!(
        !ran.get(),
        "schedule must not run the root on the caller's stack"
    );
    assert!(!done.get());

    rt.drive();
    assert!(ran.get());
    assert!(done.get(), "drive must deliver on_complete");
}

#[test]
fn drive_runs_spawned_tasks_and_yields() {
    let rt = event_loop_rt();
    let out = Rc::new(Cell::new(0));
    let out2 = out.clone();
    rt.schedule(
        async move {
            let a = tokio::spawn(async {
                tokio::task::yield_now().await;
                20
            });
            let b = tokio::spawn(async { 22 });
            a.await.unwrap() + b.await.unwrap()
        },
        move |v| out2.set(v.unwrap()),
    );
    // A yield defers to the next batch: each drive is one batch.
    for _ in 0..4 {
        if out.get() != 0 {
            break;
        }
        rt.drive();
    }
    assert_eq!(out.get(), 42);
}

#[test]
fn root_panic_is_delivered_as_join_error() {
    let rt = event_loop_rt();
    let seen = Rc::new(Cell::new(false));
    let seen2 = seen.clone();
    rt.schedule(
        async {
            panic!("root panicked");
        },
        move |out: Result<(), _>| {
            assert!(out.unwrap_err().is_panic());
            seen2.set(true);
        },
    );
    rt.drive();
    assert!(seen.get());
}

#[test]
fn host_wake_does_not_drive_inline() {
    let rt = event_loop_rt();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let done = Rc::new(Cell::new(false));

    let done2 = done.clone();
    rt.schedule(
        async move {
            rx.await.unwrap();
        },
        move |out| {
            out.unwrap();
            done2.set(true);
        },
    );
    rt.drive();
    assert!(!done.get(), "root is parked on the oneshot");

    // A wake from host context queues the task and arms a drive; it must
    // never run the task on the waker's stack.
    tx.send(()).unwrap();
    assert!(!done.get());

    rt.drive();
    assert!(done.get());
}

#[test]
fn two_runtimes_cross_wake() {
    let a = event_loop_rt();
    let b = event_loop_rt();
    let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

    let b_out = Rc::new(Cell::new(0));
    let b_out2 = b_out.clone();
    b.schedule(async move { rx.await.unwrap() }, move |v| {
        b_out2.set(v.unwrap())
    });
    b.drive();
    assert_eq!(b_out.get(), 0);

    let a_done = Rc::new(Cell::new(false));
    let a_done2 = a_done.clone();
    a.schedule(
        async move {
            tx.send(42).unwrap();
        },
        move |out| {
            out.unwrap();
            a_done2.set(true);
        },
    );
    a.drive();
    assert!(a_done.get());
    assert_eq!(
        b_out.get(),
        0,
        "the cross-runtime wake must not drive b inline"
    );

    b.drive();
    assert_eq!(b_out.get(), 42);
}

#[test]
fn timer_is_not_fired_early() {
    let rt = event_loop_rt();
    let done = Rc::new(Cell::new(false));
    let done2 = done.clone();
    rt.schedule(
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
        },
        move |out| {
            out.unwrap();
            done2.set(true);
        },
    );
    rt.drive();
    rt.drive();
    assert!(
        !done.get(),
        "a pending deadline must wait for the host timer"
    );
}

#[test]
fn block_on_is_rejected() {
    let rt = event_loop_rt();
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.local().block_on(async { 1 + 2 })
    }))
    .expect_err("block_on on an event-loop runtime must panic");
    assert!(
        panic_message(&err).contains("EventLoopRuntime"),
        "unexpected panic message: {}",
        panic_message(&err)
    );
}

#[test]
fn drive_inside_a_runtime_is_nested() {
    let outer = Builder::new_current_thread().build().unwrap();
    let rt = event_loop_rt();
    let err = outer
        .block_on(async { std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt.drive())) })
        .expect_err("drive from within a runtime must panic");
    assert!(
        panic_message(&err).contains("Cannot start a runtime from within a runtime"),
        "unexpected panic message: {}",
        panic_message(&err)
    );
}

#[test]
fn drop_cancels_in_flight_roots() {
    let rt = event_loop_rt();
    let completed = Rc::new(Cell::new(false));
    let completed2 = completed.clone();
    rt.schedule(
        async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        },
        move |_| completed2.set(true),
    );
    rt.drive();
    drop(rt);
    assert!(!completed.get());
}
