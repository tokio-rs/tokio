#![cfg(not(tokio_wasi))]

use std::{task::Context, time::Duration};

#[cfg(not(loom))]
use futures::task::noop_waker_ref;

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::loom::sync::Arc;
use crate::loom::thread;

use super::TimerEntry;

fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    #[cfg(loom)]
    return loom::future::block_on(f);

    #[cfg(not(loom))]
    {
        let rt = crate::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(f)
    }
}

fn model(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(loom)]
    loom::model(f);

    #[cfg(not(loom))]
    f();
}

fn rt(start_paused: bool) -> crate::runtime::Runtime {
    crate::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(start_paused)
        .build()
        .unwrap()
}

#[test]
fn single_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry =
                TimerEntry::new(&handle_, handle_.inner.clock.now() + Duration::from_secs(1));
            pin!(entry);

            block_on(futures::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx)
            }))
            .unwrap();
        });

        thread::yield_now();

        let handle = handle.as_time_handle();

        // This may or may not return Some (depending on how it races with the
        // thread). If it does return None, however, the timer should complete
        // synchronously.
        handle.process_at_time(handle.time_source().now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn drop_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry =
                TimerEntry::new(&handle_, handle_.inner.clock.now() + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
        });

        thread::yield_now();

        let handle = handle.as_time_handle();

        // advance 2s in the future.
        handle.process_at_time(handle.time_source().now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn change_waker() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry =
                TimerEntry::new(&handle_, handle_.inner.clock.now() + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

            block_on(futures::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx)
            }))
            .unwrap();
        });

        thread::yield_now();

        let handle = handle.as_time_handle();

        // advance 2s
        handle.process_at_time(handle.time_source().now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn reset_future() {
    model(|| {
        let finished_early = Arc::new(AtomicBool::new(false));

        let rt = rt(false);
        let handle = rt.handle();

        let handle_ = handle.clone();
        let finished_early_ = finished_early.clone();
        let start = handle.inner.clock.now();

        let jh = thread::spawn(move || {
            let entry = TimerEntry::new(&handle_, start + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

            entry.as_mut().reset(start + Duration::from_secs(2));

            // shouldn't complete before 2s
            block_on(futures::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx)
            }))
            .unwrap();

            finished_early_.store(true, Ordering::Relaxed);
        });

        thread::yield_now();

        let handle = handle.as_time_handle();

        // This may or may not return a wakeup time.
        handle.process_at_time(
            handle
                .time_source()
                .instant_to_tick(start + Duration::from_millis(1500)),
        );

        assert!(!finished_early.load(Ordering::Relaxed));

        handle.process_at_time(
            handle
                .time_source()
                .instant_to_tick(start + Duration::from_millis(2500)),
        );

        jh.join().unwrap();

        assert!(finished_early.load(Ordering::Relaxed));
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

    for i in 0..normal_or_miri(1024, 64) {
        let mut entry = Box::pin(TimerEntry::new(
            &handle,
            handle.inner.clock.now() + Duration::from_millis(i),
        ));

        let _ = entry
            .as_mut()
            .poll_elapsed(&mut Context::from_waker(noop_waker_ref()));

        entries.push(entry);
    }

    for t in 1..normal_or_miri(1024, 64) {
        handle.as_time_handle().process_at_time(t as u64);

        for (deadline, future) in entries.iter_mut().enumerate() {
            let mut context = Context::from_waker(noop_waker_ref());
            if deadline <= t {
                assert!(future.as_mut().poll_elapsed(&mut context).is_ready());
            } else {
                assert!(future.as_mut().poll_elapsed(&mut context).is_pending());
            }
        }
    }
}

#[test]
#[cfg(not(loom))]
fn poll_process_levels_targeted() {
    let mut context = Context::from_waker(noop_waker_ref());

    let rt = rt(true);
    let handle = rt.handle();

    let e1 = TimerEntry::new(
        &handle,
        handle.inner.clock.now() + Duration::from_millis(193),
    );
    pin!(e1);

    let handle = handle.as_time_handle();

    handle.process_at_time(62);
    assert!(e1.as_mut().poll_elapsed(&mut context).is_pending());
    handle.process_at_time(192);
    handle.process_at_time(192);
}
