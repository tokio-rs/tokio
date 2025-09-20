#![cfg(not(target_os = "wasi"))]

use std::future::poll_fn;
use std::{task::Context, time::Duration};

#[cfg(not(loom))]
use futures::task::noop_waker_ref;

use crate::loom::thread;
use crate::runtime::time::timer::with_current_wheel;
use crate::runtime::Handle;
use crate::sync::oneshot;

use super::Timer;

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
        crate::task::yield_now().await;
        if !exit_rx.is_empty() {
            // break the loop if the thread is exiting
            break;
        }

        // In the `block_on` context, we can get the current wheel
        // fire all timers.
        with_current_wheel(&handle.inner, |maybe_wheel| {
            let (wheel, _tx, _is_shutdown) = maybe_wheel.unwrap();
            let time = handle.inner.driver().time();
            time.process_at_time(wheel, u64::MAX); // 2 seconds
        });

        thread::yield_now();
    }
}

// This function must be called inside the `rt.block_on`.
fn process_at_time(handle: &Handle, at: u64) {
    let handle = &handle.inner;
    with_current_wheel(handle, |maybe_wheel| {
        let (wheel, _tx, _is_shutdown) = maybe_wheel.unwrap();
        let time = handle.driver().time();
        time.process_at_time(wheel, at);
    });
}

fn rt(start_paused: bool) -> crate::runtime::Runtime {
    crate::runtime::Builder::new_current_thread()
        .enable_time()
        .event_interval(1)
        .start_paused(start_paused)
        .build()
        .unwrap()
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
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
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
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

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
                let mut context = Context::from_waker(noop_waker_ref());
                if deadline <= t as usize {
                    assert!(future.as_mut().poll_elapsed(&mut context).is_ready());
                } else {
                    assert!(future.as_mut().poll_elapsed(&mut context).is_pending());
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
