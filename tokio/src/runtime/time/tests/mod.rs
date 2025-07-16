#![cfg(not(target_os = "wasi"))]

use std::{future::poll_fn, task::Context, time::Duration};

#[cfg(not(loom))]
use futures::task::noop_waker_ref;

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::loom::sync::Arc;
use crate::loom::thread;
use crate::task::yield_now;

use super::TimerEntry;

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

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn(async move {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);

                poll_fn(|cx| entry.as_mut().poll_elapsed(cx)).await.unwrap();
            });

            yield_now().await;

            let time = handle.inner.driver().time();
            let clock = handle.inner.driver().clock();

            // advance 2s
            time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

            jh.await.unwrap();
        });
    })
}

#[test]
fn drop_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn(async move {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);

                let _ = entry
                    .as_mut()
                    .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
                let _ = entry
                    .as_mut()
                    .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
            });

            yield_now().await;

            let time = handle.inner.driver().time();
            let clock = handle.inner.driver().clock();

            // advance 2s in the future.
            time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

            jh.await.unwrap();
        });
    })
}

#[test]
fn change_waker() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn(async move {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);

                let _ = entry
                    .as_mut()
                    .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

                poll_fn(|cx| entry.as_mut().poll_elapsed(cx)).await.unwrap();
            });

            yield_now().await;

            let time = handle.inner.driver().time();
            let clock = handle.inner.driver().clock();

            // advance 2s
            time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

            jh.await.unwrap();
        });
    })
}

#[test]
fn reset_future() {
    model(|| {
        let finished_early = Arc::new(AtomicBool::new(false));

        let rt = rt(false);
        let handle = rt.handle();

        let finished_early_ = finished_early.clone();
        let start = handle.inner.driver().clock().now();

        rt.block_on(async move {
            let jh = handle.spawn(async move {
                let entry = TimerEntry::new(start + Duration::from_secs(1));
                pin!(entry);

                let _ = entry
                    .as_mut()
                    .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

                entry.as_mut().reset(start + Duration::from_secs(2), true);

                // shouldn't complete before 2s
                poll_fn(|cx| entry.as_mut().poll_elapsed(cx)).await.unwrap();

                finished_early_.store(true, Ordering::Relaxed);
            });

            thread::yield_now();

            let handle = handle.inner.driver().time();

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

            jh.await.unwrap();

            assert!(finished_early.load(Ordering::Relaxed));
        });
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

    rt.block_on(async move {
        for i in 0..normal_or_miri(1024, 64) {
            let mut entry = Box::pin(TimerEntry::new(
                handle.inner.driver().clock().now() + Duration::from_millis(i),
            ));

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(noop_waker_ref()));

            entries.push(entry);
        }

        for t in 1..normal_or_miri(1024, 64) {
            handle.inner.driver().time().process_at_time(t as u64);

            for (deadline, future) in entries.iter_mut().enumerate() {
                let mut context = Context::from_waker(noop_waker_ref());
                if deadline <= t {
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

    rt.block_on(async move {
        let e1 = TimerEntry::new(handle.inner.driver().clock().now() + Duration::from_millis(193));
        pin!(e1);

        let handle = handle.inner.driver().time();

        handle.process_at_time(62);
        assert!(e1.as_mut().poll_elapsed(&mut context).is_pending());
        handle.process_at_time(192);
        handle.process_at_time(192);
    });
}

#[test]
#[cfg(not(loom))]
fn instant_to_tick_max() {
    use crate::runtime::time::entry::MAX_SAFE_MILLIS_DURATION;

    let rt = rt(true);
    let handle = rt.handle().inner.driver().time();

    let start_time = handle.time_source.start_time();
    let long_future = start_time + std::time::Duration::from_millis(MAX_SAFE_MILLIS_DURATION + 1);

    assert!(handle.time_source.instant_to_tick(long_future) <= MAX_SAFE_MILLIS_DURATION);
}
