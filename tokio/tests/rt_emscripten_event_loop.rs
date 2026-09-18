//! `LocalEventLoop` contracts that hold on the caller's stack under
//! Emscripten: `spawn_local` queues only, `drive` runs one batch without
//! suspending, wakes from host context never run tasks inline, `block_on`
//! never waits, and a pending deadline waits for the host. Plain `#[test]`s,
//! so the bodies run outside any drive. What needs the host loop itself
//! (the drives it schedules after `main` returns) is
//! `rt_emscripten_event_loop_main`.

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
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, LocalEventLoop};

fn event_loop() -> LocalEventLoop {
    Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_hosted_local_event_loop(Default::default())
        .unwrap()
}

#[test]
fn spawn_queues_and_drive_runs() {
    let el = event_loop();
    let ran = Rc::new(Cell::new(false));
    let ran2 = ran.clone();
    let jh = el.spawn_local(async move {
        ran2.set(true);
        3
    });
    assert!(!ran.get(), "spawn must not run on the caller's stack");

    el.drive();
    assert!(ran.get());
    assert!(jh.is_finished());
}

#[test]
fn drive_is_one_batch() {
    let el = event_loop();
    let turns = Rc::new(Cell::new(0));
    let t = turns.clone();
    let jh = el.spawn_local(async move {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            t.set(t.get() + 1);
        }
    });
    el.drive();
    // The task yielded back into the queue and the drive returned to the
    // host rather than running to completion; each yield defers to the next
    // batch.
    assert_eq!(turns.get(), 0);
    for _ in 0..20 {
        el.drive();
    }
    assert!(jh.is_finished());
    assert_eq!(turns.get(), 20);
}

#[test]
fn inject_leftovers_run_from_later_drives() {
    // Spawns from host context land in the inject queue; a batch that ends
    // at `event_interval` with more queued must leave a drive scheduled (on
    // the hosted loop, via its own waker), or with the runtime held alive
    // and nothing armed the process would hang. Here the drives are made by
    // hand; `rt_emscripten_event_loop_main` covers the scheduled ones.
    let el = event_loop();
    let ran = Rc::new(Cell::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.spawn_local(async move { r.set(r.get() + 1) });
    }
    el.drive();
    assert_eq!(ran.get(), 4, "one event_interval(4) batch");
    el.drive();
    assert_eq!(ran.get(), 6);
}

#[test]
fn block_on_ready_future() {
    let el = event_loop();
    assert_eq!(el.block_on(async { 1 + 2 }), Ok(3));
}

#[test]
fn block_on_drives_ready_tasks() {
    let el = event_loop();
    let out = el.block_on(async {
        let a = tokio::spawn(async { 20 });
        let b = tokio::spawn(async {
            tokio::task::yield_now().await;
            22
        });
        a.await.unwrap() + b.await.unwrap()
    });
    assert_eq!(out, Ok(42));
}

#[test]
fn block_on_would_block_on_timer() {
    let el = event_loop();
    let start = Instant::now();
    let res = el.block_on(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    assert!(res.is_err());
    assert!(start.elapsed() < Duration::from_millis(50), "must not wait");

    // The runtime is intact.
    assert_eq!(el.block_on(async { 1 }), Ok(1));
}

#[test]
fn task_panic_is_a_join_error() {
    let el = event_loop();
    let jh = el.spawn_local(async {
        panic!("task panicked");
    });
    el.drive();
    assert!(el.block_on(async { jh.await.unwrap_err().is_panic() }) == Ok(true));
}

#[test]
fn host_wake_does_not_drive_inline() {
    let el = event_loop();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let done = Rc::new(Cell::new(false));
    let done2 = done.clone();
    el.spawn_local(async move {
        rx.await.unwrap();
        done2.set(true);
    });
    el.drive();
    assert!(!done.get(), "task is parked on the oneshot");

    // A wake from host context queues the task and schedules a drive; it
    // must never run the task on the waker's stack.
    tx.send(()).unwrap();
    assert!(!done.get());

    el.drive();
    assert!(done.get());
}

#[test]
fn two_event_loops_cross_wake() {
    let a = event_loop();
    let b = event_loop();
    let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

    let b_out = Rc::new(Cell::new(0));
    let b_out2 = b_out.clone();
    b.spawn_local(async move { b_out2.set(rx.await.unwrap()) });
    b.drive();
    assert_eq!(b_out.get(), 0);

    let a_jh = a.spawn_local(async move {
        tx.send(42).unwrap();
    });
    a.drive();
    assert!(a_jh.is_finished());
    assert_eq!(
        b_out.get(),
        0,
        "the cross-loop wake must not drive b inline"
    );

    b.drive();
    assert_eq!(b_out.get(), 42);
}

#[test]
fn timer_is_not_fired_early() {
    let el = event_loop();
    let jh = el.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    el.drive();
    el.drive();
    assert!(
        !jh.is_finished(),
        "a pending deadline must wait for the host timer"
    );
}

#[test]
fn drive_inside_a_runtime_is_nested() {
    let outer = Builder::new_current_thread().build().unwrap();
    let el = event_loop();
    let err = outer
        .block_on(async { std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el.drive())) })
        .expect_err("drive from within a runtime must panic");
    let msg = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("Cannot start a runtime from within a runtime"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn drop_cancels_in_flight_tasks() {
    struct Flag(Rc<Cell<bool>>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let el = event_loop();
    let dropped = Rc::new(Cell::new(false));
    let flag = Flag(dropped.clone());
    el.spawn_local(async move {
        let _flag = flag;
        tokio::time::sleep(Duration::from_millis(10)).await;
    });
    el.drive();
    assert!(!dropped.get());
    drop(el);
    assert!(dropped.get());
}
