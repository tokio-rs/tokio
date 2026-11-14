//! Hosted `LocalEventLoop` contracts that hold on the caller's stack under
//! Emscripten: `spawn_local` queues only, `tick` runs one batch without
//! suspending, wakes from host context never run tasks inline, ready work
//! left by a tick schedules one immediate, and a pending deadline waits for
//! the host. Plain `#[test]`s, so the bodies run outside any tick. What
//! needs the host loop itself (the ticks it schedules after `main` returns)
//! is `rt_emscripten_event_loop_main`.

#![cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable,
    feature = "rt",
    feature = "time",
    feature = "sync",
    feature = "net"
))]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use tokio::runtime::{Builder, LocalEventLoop};

const ZERO: Option<Duration> = Some(Duration::ZERO);

fn event_loop() -> LocalEventLoop {
    Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_hosted_local_event_loop(Default::default())
        .unwrap()
}

extern "C" {
    fn emscripten_run_script_int(script: *const std::ffi::c_char) -> i32;
}

fn host_count(name: &str) -> i32 {
    let script = std::ffi::CString::new(format!("Module.{name}")).unwrap();
    // SAFETY: a NUL-terminated script evaluated on the host.
    unsafe { emscripten_run_script_int(script.as_ptr()) }
}

/// Immediates the host has seen: a tick's follow-up is one.
fn immediates() -> i32 {
    host_count("tokioImmediates")
}

// A tick that leaves ready work schedules one follow-up tick, as an
// immediate so the host gets a turn between batches; a second tick with
// work still left does not schedule another while it is pending.
#[test]
fn ready_work_schedules_one_immediate() {
    let mut el = event_loop();
    let ran = Rc::new(Cell::new(0));
    for _ in 0..10 {
        let r = ran.clone();
        el.spawn_local(async move { r.set(r.get() + 1) });
    }
    let before = immediates();
    assert_eq!(el.tick(4), 4);
    assert_eq!(el.next_sleep(), ZERO);
    assert_eq!(immediates() - before, 1, "one follow-up for the leftovers");
    assert_eq!(el.tick(4), 4);
    assert_eq!(ran.get(), 8);
    assert_eq!(el.next_sleep(), ZERO);
    assert_eq!(immediates() - before, 1, "still pending, not re-armed");
    assert_eq!(el.tick(4), 2);
    assert_eq!(ran.get(), 10);
    assert_eq!(el.next_sleep(), None);
    assert_eq!(immediates() - before, 1);
}

#[test]
fn spawn_queues_and_tick_runs() {
    let mut el = event_loop();
    let ran = Rc::new(Cell::new(false));
    let ran2 = ran.clone();
    let jh = el.spawn_local(async move {
        ran2.set(true);
        3
    });
    assert!(!ran.get(), "spawn must not run on the caller's stack");
    assert_eq!(el.next_sleep(), ZERO);

    assert_eq!(el.tick(64), 1);
    assert!(ran.get());
    assert!(jh.is_finished());
    assert_eq!(el.next_sleep(), None);
}

#[test]
fn tick_is_one_batch() {
    let mut el = event_loop();
    let turns = Rc::new(Cell::new(0));
    let t = turns.clone();
    let jh = el.spawn_local(async move {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            t.set(t.get() + 1);
        }
    });
    el.tick(64);
    // The task yielded back into the queue and the tick returned to the
    // host rather than running to completion; each yield defers to the next
    // batch.
    assert_eq!(turns.get(), 0);
    assert_eq!(el.next_sleep(), ZERO);
    for _ in 0..20 {
        assert_eq!(el.tick(64), 1);
    }
    assert!(jh.is_finished());
    assert_eq!(turns.get(), 20);
}

#[test]
fn inject_leftovers_run_from_later_ticks() {
    // Spawns from host context land in the inject queue; a batch that ends
    // at its limit with more queued must leave a tick scheduled, or with the
    // runtime held alive and nothing armed the process would hang. Here the
    // ticks are made by hand; `rt_emscripten_event_loop_main` covers the
    // scheduled ones.
    let mut el = event_loop();
    let ran = Rc::new(Cell::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.spawn_local(async move { r.set(r.get() + 1) });
    }
    assert_eq!(el.tick(4), 4);
    assert_eq!(el.next_sleep(), ZERO);
    assert_eq!(el.tick(4), 2);
    assert_eq!(el.next_sleep(), None);
}

#[test]
fn task_panic_is_a_join_error() {
    let mut el = event_loop();
    let jh = el.spawn_local(async {
        panic!("task panicked");
    });
    let is_panic = Rc::new(Cell::new(false));
    let is_panic2 = is_panic.clone();
    let probe = el.spawn_local(async move { is_panic2.set(jh.await.unwrap_err().is_panic()) });
    while !probe.is_finished() {
        el.tick(64);
    }
    assert!(is_panic.get());
}

#[test]
fn host_wake_does_not_tick_inline() {
    let mut el = event_loop();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let done = Rc::new(Cell::new(false));
    let done2 = done.clone();
    el.spawn_local(async move {
        rx.await.unwrap();
        done2.set(true);
    });
    el.tick(64);
    assert!(!done.get(), "task is parked on the oneshot");
    assert_eq!(el.next_sleep(), None);

    // A wake from host context queues the task and wakes the reactor; it
    // must never run the task on the waker's stack.
    tx.send(()).unwrap();
    assert!(!done.get());
    assert_eq!(el.next_sleep(), ZERO);

    el.tick(64);
    assert!(done.get());
}

#[test]
fn two_event_loops_cross_wake() {
    let mut a = event_loop();
    let mut b = event_loop();
    let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

    let b_out = Rc::new(Cell::new(0));
    let b_out2 = b_out.clone();
    b.spawn_local(async move { b_out2.set(rx.await.unwrap()) });
    b.tick(64);
    assert_eq!(b_out.get(), 0);

    let a_jh = a.spawn_local(async move {
        tx.send(42).unwrap();
    });
    a.tick(64);
    assert!(a_jh.is_finished());
    assert_eq!(b_out.get(), 0, "the cross-loop wake must not tick b inline");

    b.tick(64);
    assert_eq!(b_out.get(), 42);
}

#[test]
fn timer_is_not_fired_early() {
    let mut el = event_loop();
    let jh = el.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    el.tick(64);
    let next = el.next_sleep().expect("the deadline");
    assert!(Duration::from_millis(40) < next && next <= Duration::from_millis(51));
    el.tick(64);
    assert!(
        !jh.is_finished(),
        "a pending deadline must wait for the host timer"
    );
}

#[test]
fn tick_inside_a_runtime_is_nested() {
    let outer = Builder::new_current_thread().build().unwrap();
    let mut el = event_loop();
    let err = outer
        .block_on(async { std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el.tick(1))) })
        .expect_err("tick from within a runtime must panic");
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

    let mut el = event_loop();
    let dropped = Rc::new(Cell::new(false));
    let flag = Flag(dropped.clone());
    el.spawn_local(async move {
        let _flag = flag;
        tokio::time::sleep(Duration::from_millis(10)).await;
    });
    el.tick(64);
    assert!(!dropped.get());
    drop(el);
    assert!(dropped.get());
}
