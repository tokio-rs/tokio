#![cfg(not(target_os = "wasi"))]

use std::sync::Barrier;
use std::{future::poll_fn, task::Context, time::Duration};

use tokio_test::assert_pending;

#[cfg(not(loom))]
use futures::task::noop_waker_ref;

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::loom::sync::Arc;

use super::TimerEntry;

fn model(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(loom)]
    loom::model(f);

    #[cfg(not(loom))]
    f();
}

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

fn cx() -> Context<'static> {
    Context::from_waker(noop_waker_ref())
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
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn_blocking(move || {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);
                assert_pending!(entry.as_mut().poll_elapsed(&mut cx()));
                // Make sure the previous poll_elapsed was called, so the runtime handle
                // was cloned and stored in the entry.
                barrier_clone.wait();
                block_on(poll_fn(|cx| entry.as_mut().poll_elapsed(cx))).unwrap();
            });

            // Making sure the first poll_elapsed was called, so the runtime handle
            // was cloned and stored in the entry.
            barrier.wait();

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
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn_blocking(move || {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);

                assert_pending!(entry.as_mut().poll_elapsed(&mut cx()));
                assert_pending!(entry.as_mut().poll_elapsed(&mut cx()));
                // Make sure the previous poll_elapsed was called, so the runtime handle
                // was cloned and stored in the entry.
                barrier_clone.wait();
            });

            // Making sure the first poll_elapsed was called, so the runtime handle
            // was cloned and stored in the entry.
            barrier.wait();

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
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        rt.block_on(async move {
            let handle_ = handle.clone();
            let jh = handle.spawn_blocking(move || {
                let entry =
                    TimerEntry::new(handle_.inner.driver().clock().now() + Duration::from_secs(1));
                pin!(entry);

                assert_pending!(entry.as_mut().poll_elapsed(&mut cx()));
                // Make sure the previous poll_elapsed was called, so the runtime handle
                // was cloned and stored in the entry.
                barrier_clone.wait();
                block_on(poll_fn(|cx| entry.as_mut().poll_elapsed(cx))).unwrap();
            });

            // Making sure the first poll_elapsed was called, so the runtime handle
            // was cloned and stored in the entry.
            barrier.wait();

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

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        rt.block_on(async move {
            let jh = handle.spawn_blocking(move || {
                let entry = TimerEntry::new(start + Duration::from_secs(1));
                pin!(entry);

                assert_pending!(entry.as_mut().poll_elapsed(&mut cx()));
                // Make sure the previous poll_elapsed was called, so the runtime handle
                // was cloned and stored in the entry.
                barrier_clone.wait();
                entry.as_mut().reset(start + Duration::from_secs(2), true);

                // shouldn't complete before 2s
                block_on(poll_fn(|cx| entry.as_mut().poll_elapsed(cx))).unwrap();

                finished_early_.store(true, Ordering::Relaxed);
            });

            // Making sure the first poll_elapsed was called, so the runtime handle
            // was cloned and stored in the entry.
            barrier.wait();

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

            let _ = entry.as_mut().poll_elapsed(&mut cx());
            entries.push(entry);
        }

        for t in 1..normal_or_miri(1024, 64) {
            handle.inner.driver().time().process_at_time(t as u64);

            for (deadline, future) in entries.iter_mut().enumerate() {
                if deadline <= t {
                    assert!(future.as_mut().poll_elapsed(&mut cx()).is_ready());
                } else {
                    assert!(future.as_mut().poll_elapsed(&mut cx()).is_pending());
                }
            }
        }
    });
}

#[test]
#[cfg(not(loom))]
fn poll_process_levels_targeted() {
    let rt = rt(true);
    let handle = rt.handle();

    rt.block_on(async move {
        let e1 = TimerEntry::new(handle.inner.driver().clock().now() + Duration::from_millis(193));
        pin!(e1);

        let handle = handle.inner.driver().time();

        handle.process_at_time(62);
        assert_pending!(e1.as_mut().poll_elapsed(&mut cx()));
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
