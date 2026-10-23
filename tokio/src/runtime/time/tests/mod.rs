#![cfg(all(not(target_os = "wasi"), not(target_os = "emscripten")))]

use std::task::Context;

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

#[cfg(feature = "test-util")]
fn paused_driver() -> (
    crate::runtime::driver::Driver,
    Arc<crate::runtime::driver::Handle>,
) {
    use crate::runtime::driver::{Cfg, Driver};

    let (driver, handle) = Driver::new(Cfg {
        enable_io: false,
        enable_time: true,
        enable_pause_time: true,
        start_paused: true,
        nevents: 1,
        nevents_busy: None,
        timer_flavor: crate::runtime::TimerFlavor::Traditional,
    })
    .unwrap();
    (driver, Arc::new(handle))
}

fn rt(start_paused: bool) -> crate::runtime::Runtime {
    crate::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(start_paused)
        .build()
        .unwrap()
}

#[test]
#[cfg(feature = "test-util")]
fn inhibited_clock_bounds_timer_wait() {
    assert_eq!(
        super::park_timeout_duration(
            crate::time::ClockState::PausedInhibited,
            std::time::Duration::from_nanos(1),
            None,
        ),
        std::time::Duration::from_millis(1),
    );
}

#[test]
#[cfg(feature = "test-util")]
fn park_timeout_preserves_precision_and_limits() {
    use crate::time::ClockState::{Paused, PausedInhibited, Running};
    use std::time::Duration;

    let zero = Duration::ZERO;
    let short = Duration::from_nanos(1);
    let limit = Duration::from_nanos(100);
    let tick = Duration::from_millis(1);
    let long = Duration::from_micros(1500);
    let cases = [
        (Running, short, None, short),
        (Paused, short, None, short),
        (PausedInhibited, short, None, tick),
        (Running, zero, None, zero),
        (Paused, zero, None, zero),
        (PausedInhibited, zero, None, zero),
        (PausedInhibited, long, None, long),
        (PausedInhibited, short, Some(zero), zero),
        (PausedInhibited, short, Some(limit), limit),
        (PausedInhibited, long, Some(limit), limit),
        (PausedInhibited, short, Some(short), short),
        (Running, long, Some(limit), limit),
        (Paused, long, Some(limit), limit),
    ];

    for (state, duration, limit, expected) in cases {
        assert_eq!(
            super::park_timeout_duration(state, duration, limit),
            expected,
            "state: {state:?}, duration: {duration:?}, limit: {limit:?}",
        );
    }
}

#[test]
#[cfg(feature = "test-util")]
fn clock_state_tracks_nested_inhibition() {
    model(|| {
        use crate::time::{Clock, ClockState};

        let clock = Clock::new(true, false);
        assert_eq!(clock.state(), ClockState::Running);

        clock.inhibit_auto_advance();
        assert_eq!(clock.state(), ClockState::Running);

        clock.pause().unwrap();
        assert_eq!(clock.state(), ClockState::PausedInhibited);

        clock.inhibit_auto_advance();
        clock.allow_auto_advance();
        assert_eq!(clock.state(), ClockState::PausedInhibited);

        clock.allow_auto_advance();
        assert_eq!(clock.state(), ClockState::Paused);
    });
}

#[test]
#[cfg(feature = "test-util")]
fn inhibition_release_unparks_after_clock_snapshot() {
    model(|| {
        use crate::time::ClockState;

        let (mut driver, handle) = paused_driver();
        handle.clock().inhibit_auto_advance();
        handle.clock().inhibit_auto_advance();

        for state in [ClockState::PausedInhibited, ClockState::Paused] {
            let release_handle = handle.clone();
            let release = thread::spawn(move || {
                release_handle.clock().allow_auto_advance();
                release_handle.unpark();
            });

            if handle.clock().state() == ClockState::PausedInhibited {
                driver.park(&handle);
            }

            release.join().unwrap();
            assert_eq!(handle.clock().state(), state);
            assert!(handle.time().did_wake());
        }

        driver.shutdown(&handle);
    });
}

#[test]
fn single_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let inner = handle.inner.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new();
            pin!(entry);
            entry.as_mut().init(&inner, inner.driver().now() + 1000);

            block_on(std::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx, &inner)
            }))
            .unwrap();
            entry.cancel(&inner);
        });

        thread::yield_now();

        let time = handle.inner.driver().time();
        let clock = handle.inner.driver().clock();

        // advance 2s
        time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn drop_timer() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let inner = handle.inner.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new();
            pin!(entry);
            entry.as_mut().init(&inner, inner.driver().now() + 1000);

            let _ = entry.as_mut().poll_elapsed(
                &mut Context::from_waker(futures::task::noop_waker_ref()),
                &inner,
            );
            let _ = entry.as_mut().poll_elapsed(
                &mut Context::from_waker(futures::task::noop_waker_ref()),
                &inner,
            );
            entry.cancel(&inner);
        });

        thread::yield_now();

        let time = handle.inner.driver().time();
        let clock = handle.inner.driver().clock();

        // advance 2s in the future.
        time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn change_waker() {
    model(|| {
        let rt = rt(false);
        let handle = rt.handle();

        let inner = handle.inner.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new();
            pin!(entry);
            entry.as_mut().init(&inner, inner.driver().now() + 1000);

            let _ = entry.as_mut().poll_elapsed(
                &mut Context::from_waker(futures::task::noop_waker_ref()),
                &inner,
            );

            block_on(std::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx, &inner)
            }))
            .unwrap();
            entry.cancel(&inner);
        });

        thread::yield_now();

        let time = handle.inner.driver().time();
        let clock = handle.inner.driver().clock();

        // advance 2s
        time.process_at_time(time.time_source().now(clock) + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn reset_future() {
    model(|| {
        let finished_early = Arc::new(AtomicBool::new(false));

        let rt = rt(false);
        let handle = rt.handle();

        let inner = handle.clone().inner;
        let finished_early_ = finished_early.clone();
        let start = handle.inner.driver().now();

        let jh = thread::spawn(move || {
            let entry = TimerEntry::new();
            pin!(entry);
            entry.as_mut().init(&inner, start + 1000);

            let _ = entry.as_mut().poll_elapsed(
                &mut Context::from_waker(futures::task::noop_waker_ref()),
                &inner,
            );

            entry.as_mut().reset(&inner, start + 2000);

            // shouldn't complete before 2s
            block_on(std::future::poll_fn(|cx| {
                entry.as_mut().poll_elapsed(cx, &inner)
            }))
            .unwrap();

            finished_early_.store(true, Ordering::Relaxed);
            entry.cancel(&inner);
        });

        thread::yield_now();

        let handle = handle.inner.driver().time();

        handle.process_at_time(start + 1500);

        assert!(!finished_early.load(Ordering::Relaxed));

        handle.process_at_time(start + 2500);

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
        let mut entry = Box::pin(TimerEntry::new());
        entry
            .as_mut()
            .init(&handle.inner, handle.inner.driver().now() + i);

        let _ = entry
            .as_mut()
            .poll_elapsed(&mut Context::from_waker(noop_waker_ref()), &handle.inner);

        entries.push(entry);
    }

    for t in 1..normal_or_miri(1024, 64) {
        handle.inner.driver().time().process_at_time(t as u64);

        for (deadline, future) in entries.iter_mut().enumerate() {
            let mut context = Context::from_waker(noop_waker_ref());
            if deadline <= t {
                assert!(future
                    .as_mut()
                    .poll_elapsed(&mut context, &handle.inner)
                    .is_ready());
            } else {
                assert!(future
                    .as_mut()
                    .poll_elapsed(&mut context, &handle.inner)
                    .is_pending());
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

    let e1 = TimerEntry::new();
    pin!(e1);
    e1.as_mut()
        .init(&handle.inner, handle.inner.driver().now() + 193);

    let time = handle.inner.driver().time();

    time.process_at_time(62);
    assert!(e1
        .as_mut()
        .poll_elapsed(&mut context, &handle.inner)
        .is_pending());
    time.process_at_time(192);
    time.process_at_time(192);
    e1.cancel(&handle.inner);
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
