#![cfg(not(target_os = "wasi"))]

use std::future::poll_fn;
use std::{task::Context, time::Duration};
use futures::task::noop_waker_ref;

use crate::loom::thread;
use crate::runtime::scheduler::util::time::process_registration_queue;
use crate::runtime::time::timer::with_current_time_context2;
use crate::runtime::time::WakeQueue;
use crate::runtime::Handle;
use crate::sync::oneshot;

use super::Timer;

const EVENT_INTERVAL: u32 = 1;

fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    #[cfg(loom)]
    return loom::future::block_on(f);

    #[cfg(not(loom))]
    {
        let rt = crate::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(f)
        // futures::executor::block_on(f)
    }
}

fn model(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(loom)]
    loom::model(f);

    #[cfg(not(loom))]
    f();
}

async fn fire_all_timers(handle: &Handle, exit_rx: oneshot::Receiver<()>) {
    loop {
        // Keep the worker thread busy, so that it can process injected
        // timers.
        assert_eq!(EVENT_INTERVAL, 1);
        crate::task::yield_now().await;
        if !exit_rx.is_empty() {
            // break the loop if the thread is exiting
            break;
        }

        let mut wake_queue = WakeQueue::new();

        // In the `block_on` context, we can get the current wheel
        // fire all timers.
        with_current_time_context2(&handle.inner, |maybe_time_cx2| {
            let time_cx2 = maybe_time_cx2.unwrap();

            process_registration_queue(
                &mut time_cx2.registration_queue,
                &mut time_cx2.wheel,
                &time_cx2.canc_tx,
                &mut wake_queue,
            );

            let time = handle.inner.driver().time();
            time.process_at_time(&mut time_cx2.wheel, u64::MAX, &mut wake_queue);
        });

        wake_queue.wake_all();

        thread::yield_now();
    }
}

// This function must be called inside the `rt.block_on`.
fn process_at_time(handle: &Handle, at: u64) {
    let handle = &handle.inner;

    with_current_time_context2(handle, |maybe_time_cx2| {
        let time_cx2 = maybe_time_cx2.unwrap();

        let mut wake_queue = WakeQueue::new();
        process_registration_queue(
            &mut time_cx2.registration_queue,
            &mut time_cx2.wheel,
            &time_cx2.canc_tx,
            &mut wake_queue,
        );

        let time = handle.driver().time();
        time.process_at_time(&mut time_cx2.wheel, at, &mut wake_queue);
        wake_queue.wake_all();
    });
}

fn rt(start_paused: bool) -> crate::runtime::Runtime {
    crate::runtime::Builder::new_current_thread()
        .enable_time()
        .event_interval(EVENT_INTERVAL)
        .start_paused(start_paused)
        .build()
        .unwrap()
}

fn noop_cx() -> Context<'static> {
    Context::from_waker(noop_waker_ref())
}

#[test]
fn single_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();
        let (exit_tx, exit_rx) = oneshot::channel();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = Timer::new(
                handle_.inner.clone(),
                handle_.inner.driver().clock().now() + Duration::from_secs(1),
            );
            pin!(entry);

            block_on(poll_fn(|cx| entry.as_mut().poll_elapsed(cx)));
            exit_tx.send(()).unwrap();
        });

        rt.block_on(async move {
            fire_all_timers(handle, exit_rx).await;
        });

        jh.join().unwrap();
    })
}

#[test]
fn drop_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();
        let (exit_tx, exit_rx) = oneshot::channel();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = Timer::new(
                handle_.inner.clone(),
                handle_.inner.driver().clock().now() + Duration::from_secs(1),
            );
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut noop_cx());
            let _ = entry
                .as_mut()
                .poll_elapsed(&mut noop_cx());
            exit_tx.send(()).unwrap();
        });

        rt.block_on(async move {
            fire_all_timers(handle, exit_rx).await;
        });

        jh.join().unwrap();
    })
}

#[test]
fn change_waker() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();
        let (exit_tx, exit_rx) = oneshot::channel();
        let (change_waker_tx, change_waker_rx) = oneshot::channel();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = Timer::new(
                handle_.inner.clone(),
                handle_.inner.driver().clock().now() + Duration::from_secs(1),
            );
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut noop_cx());

            // At this point, we cannot let worker thread to wake up
            // the timer because the waker is a noop.
            // Let's say the timer has been woken up at this point,
            // the following poll is basically polling a future that has completed
            // (already returned `Ready`),which is not encouraged.

            let mut maybe_change_waker_tx = Some(change_waker_tx);
            block_on(poll_fn(|cx| {
                let p = entry.as_mut().poll_elapsed(cx);
                if let Some(tx) = maybe_change_waker_tx.take() {
                    // notify the worker thread that the waker is useable now
                    tx.send(()).unwrap();
                }
                p
            }));

            // notify the worker thread to exit
            exit_tx.send(()).unwrap();
        });

        change_waker_rx.blocking_recv().unwrap();

        rt.block_on(async move {
            fire_all_timers(handle, exit_rx).await;
        });

        jh.join().unwrap();
    })
}

#[cfg(not(loom))]
fn normal_or_miri<T>(normal: T, miri: T) -> T {
    if cfg!(miri) {
        miri
    } else {
        normal
    }
}

#[test]
#[cfg(not(loom))]
fn poll_process_levels() {
    let rt = rt(true);
    let handle = rt.handle();

    let mut entries = vec![];

    rt.block_on(async {
        for i in 0..normal_or_miri(1024, 64) {
            let mut entry = Box::pin(Timer::new(
                handle.inner.clone(),
                handle.inner.driver().clock().now() + Duration::from_millis(i),
            ));

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(noop_waker_ref()));

            entries.push(entry);
        }

        for t in 1..normal_or_miri(1024, 64) {
            process_at_time(handle, t);

            for (deadline, future) in entries.iter_mut().enumerate() {
                if deadline <= t as usize {
                    assert!(future.as_mut().poll_elapsed(&mut noop_cx()).is_ready());
                } else {
                    assert!(future.as_mut().poll_elapsed(&mut noop_cx()).is_pending());
                }
            }
        }
    });
}

#[test]
#[cfg(not(loom))]
fn poll_process_levels_targeted() {
    let mut context = Context::from_waker(noop_waker_ref());

    let rt = rt(true);
    let handle = rt.handle();

    rt.block_on(async {
        let e1 = Timer::new(
            handle.inner.clone(),
            handle.inner.driver().clock().now() + Duration::from_millis(193),
        );
        pin!(e1);

        process_at_time(handle, 62);
        assert!(e1.as_mut().poll_elapsed(&mut context).is_pending());
        process_at_time(handle, 192);
        process_at_time(handle, 192);
    })
}

#[test]
fn cancel_in_the_same_rt() {
    model(|| {
        let rt = rt(false);

        rt.block_on(async {
            let handle = rt.handle();
            let mut timer = Box::pin(Timer::new(
                handle.inner.clone(),
                handle.inner.driver().clock().now() + Duration::from_secs(1),
            ));
            let poll = timer.as_mut().poll_elapsed(&mut noop_cx());
            assert!(poll.is_pending());
            drop(timer);

            // Since the event interval is 1, yield 3 times to ensure
            // the registration queue and cancellation queue are processed.
            assert_eq!(EVENT_INTERVAL, 1);
            crate::task::yield_now().await;
            crate::task::yield_now().await;
            crate::task::yield_now().await;
        });
    })
}

#[test]
fn cancel_in_the_different_rt() {
    model(|| {
        let rt1 = rt(false);
        let rt2 = rt(false);

        let timer = rt1.block_on(async {
            let handle = rt1.handle();
            let mut timer = Box::pin(Timer::new(
                handle.inner.clone(),
                handle.inner.driver().clock().now() + Duration::from_secs(1),
            ));
            let poll = timer.as_mut().poll_elapsed(&mut noop_cx());
            assert!(poll.is_pending());
            timer
        });

        rt2.block_on(async {
            drop(timer);
        });

        rt1.block_on(async {
            // Since the event interval is 1, yield 3 times to ensure
            // the registration queue and cancellation queue are processed.
            assert_eq!(EVENT_INTERVAL, 1);
            crate::task::yield_now().await;
            crate::task::yield_now().await;
            crate::task::yield_now().await;
        });
    })
}

#[test]
fn cancel_outside_of_rt() {
    model(|| {
        let rt = rt(false);

        let timer = rt.block_on(async {
            let handle = rt.handle();
            let mut timer = Box::pin(Timer::new(
                handle.inner.clone(),
                handle.inner.driver().clock().now() + Duration::from_secs(1),
            ));
            let poll = timer.as_mut().poll_elapsed(&mut noop_cx());
            assert!(poll.is_pending());
            timer
        });

        drop(timer);

        rt.block_on(async {
            // Since the event interval is 1, yield 3 times to ensure
            // the registration queue and cancellation queue are processed.
            assert_eq!(EVENT_INTERVAL, 1);
            crate::task::yield_now().await;
            crate::task::yield_now().await;
            crate::task::yield_now().await;
        });
    })
}

#[test]
fn cancel_in_different_thread() {
    model(|| {
        let rt = rt(false);

        let timer = rt.block_on(async {
            let handle = rt.handle();
            let mut timer = Box::pin(Timer::new(
                handle.inner.clone(),
                handle.inner.driver().clock().now() + Duration::from_secs(1),
            ));
            let poll = timer.as_mut().poll_elapsed(&mut noop_cx());
            assert!(poll.is_pending());
            timer
        });

        let jh = thread::spawn(move || {
            drop(timer);
        });

        rt.block_on(async {
            // Since the event interval is 1, yield 3 times to ensure
            // the registration queue and cancellation queue are processed.
            assert_eq!(EVENT_INTERVAL, 1);
            crate::task::yield_now().await;
            crate::task::yield_now().await;
            crate::task::yield_now().await;
        });

        jh.join().unwrap();
    })
}
