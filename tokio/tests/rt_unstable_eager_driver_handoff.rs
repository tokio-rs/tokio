#![cfg(all(feature = "rt-multi-thread", feature = "time"))]

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// Test that, without `enable_eager_driver_handoff`, we can reliably reproduce
/// a deadlock when a task blocks indefinitely. If this test fails, it means
/// that the test `eager_driver_handoff_fixes_deadlock` is not actually testing
/// a condition that can deadlock the runtime.
#[test]
fn deadlocks_consistently() {
    let rt = rt_builder().build().unwrap();
    assert_eq!(
        do_test(rt),
        Err(RecvTimeoutError::Timeout),
        "runtime did not deadlock! the `eager_driver_handoff_fixes_deadlock` \
         test may no longer reproduce the bug it is intended to test a fix \
         for!",
    );
}

/// This is the one that actually tests whether eager driver handoff works as
/// expected: it runs the same reproducer as `deadlocks_consistently` a single
/// timebut with `enable_eager_driver_handoff` enabled. If this test fails, it
/// means that the eager driver handoff fix is not working as expected.
#[test]
#[cfg(tokio_unstable)]
fn eager_driver_handoff_fixes_deadlock() {
    let rt = rt_builder().enable_eager_driver_handoff().build().unwrap();
    assert_eq!(
        do_test(rt),
        Ok(()),
        "the runtime should not deadlock because the driver is \
         eagerly handed off"
    );
}

/// Reproduces a deadlock occurring when the worker thread holding the I/O or
/// time driver runs a task that blocks that worker until another task, which is
/// *waiting on the I/O or time driver*, performs an action that unblocks it.
/// This general class of problem is described in:
/// <https://github.com/tokio-rs/tokio/issues/4730>.
///
/// The deadlock occurs as follows:
///
/// 1. Both worker threads are idle. Worker A parks on the time driver (holding
///    the driver lock), Worker B parks on a condvar.
/// 2. A timer fires. Worker A (the driver holder) wakes up and processes the
///    timer event, placing the woken task in its run queue.
/// 3. Worker A begins polling the task. The task blocks the worker thread
///    until it is woken by the other task, so Worker A never returns to
///    poll the driver.
/// 4. Worker B remains parked on the condvar indefinitely — nobody calls
///    `unpark` on it, so it never wakes to take over the driver.
/// 5. Subsequent timers are never woken because no worker is polling the
///    time driver. The runtime is wedged.
///
/// To trigger this reliably, the test uses two tasks with **staggered** timers:
///
/// - The "bad" task's timer fires first and **alone**, so only one task is in
///   the run queue when the driver-holding worker wakes. Because
///   `should_notify_others()` checks `run_queue.len() > 1`, it returns `false`
///   and Worker B is **not** woken.
/// - The "good" task's timer fires much later, well after the bad task has
///   blocked Worker A.
///
/// If both timers fired at the same time (e.g., both 10ms), the timer would
/// (probably) process both events in one rotation of the timer wheel, placing 2
/// tasks in the queue. `should_notify_others()` would return `true`, waking
/// Worker B and defeating the deadlock.
///
/// The specific scenario we've constructed here is, admittedly, somewhat
/// contrived: most Tokio applications aren't going to spawn tasks that attempt
/// to communicate using a `std::sync::mpsc` blocking channel. Instead, you can
/// imagine the blocking channel as a stand-in for some other operation that
/// blocks a thread until some event occurs, which is triggered by an operation
/// performed by the other task, which is waiting on an asynchronous event
/// before performing it. The task that blocks the runtime could, for example,
/// be waiting on a blocking syscall that completes only when the other task
/// does something.
fn do_test(rt: tokio::runtime::Runtime) -> Result<(), RecvTimeoutError> {
    // A blocking MPSC from the standard library is used to wait for the test to
    // complete within a reasonable amount of time, so that we can determine
    // whether or not the runtime has deadlocked.
    let (done_tx, done_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        rt.block_on(async {
            // Note that we use a *blocking* MPSC from the standard library
            // here, since the entire purpose of the channel is to temporarily
            // block a worker thread when calling `recv`.
            let (deadlock_tx, deadlock_rx) = std::sync::mpsc::channel();

            // Task 1 ("bad"): sleep briefly so both workers settle into parked
            // state, then block the worker thread. Because the timer is
            // processed by the driver-holding worker, this task will always
            // wake up on the worker holding the time driver before it decides
            // to block.
            let bad_task = tokio::spawn(async move {
                // Sleep for a bit to ensure that we end up on the worker
                // holding the time driver.
                tokio::time::sleep(Duration::from_millis(10)).await;
                // Okay, we have now definitely woken up on the worker
                // thread that polled the time driver. Now, block this
                // worker thread until woken by the *other* task. If the
                // other task's timer is allowed to complete, this task will
                // be unblocked and the runtime will keep running.
                //
                // However, if *this* task blocking the thread prevents the
                // timer from ever firing, the runtime will be wedged forever.
                deadlock_rx.recv().unwrap();
            });

            // Task 2 ("good"): its timer fires well after the bad task has
            // blocked the driver-holding worker. If the driver is wedged, this
            // sleep never completes and `block_on` hangs.
            tokio::spawn(async move {
                // Now, try to wait for a short timer. If the driver is not
                // permanently stuck, this should be okay.
                tokio::time::sleep(Duration::from_millis(1000)).await;
                deadlock_tx.send(()).unwrap();
            })
            .await
            .unwrap();

            bad_task.await.unwrap();
        });

        done_tx.send(()).unwrap();
    });

    done_rx.recv_timeout(Duration::from_secs(20))
}

/// Base runtime builder for both tests in this module: two worker threads, time
/// driver enabled. This returns a `Builder`, rather than a `Runtime`, so that
/// the `eager_driver_handoff_fixes_deadlock` test can configure the runtime to
/// enable eager driver handoff before building it.
fn rt_builder() -> tokio::runtime::Builder {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_time().worker_threads(2);
    builder
}
