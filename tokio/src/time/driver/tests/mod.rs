use std::{task::Context, time::Duration};

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::loom::sync::{Arc, Mutex};
use crate::loom::thread;

use super::{InternalHandle, TimeSource, TimeUnpark, TimerEntry};

fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    #[cfg(loom)]
    return loom::future::block_on(f);

    #[cfg(not(loom))]
    return futures::executor::block_on(f);
}

fn model(f: impl Fn() -> () + Send + Sync + 'static) {
    #[cfg(loom)]
    loom::model(f);

    #[cfg(not(loom))]
    f();
}

#[test]
fn single_timer() {
    model(|| {
        let clock = crate::time::clock::Clock::new();
        let time_source = super::ClockTime::new(clock.clone());

        let inner = super::Inner::new(time_source.clone(), TimeUnpark::mock());
        let handle = InternalHandle::new(Arc::new(Mutex::new(inner)));

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new(&handle_, clock.now() + Duration::from_secs(1));
            pin!(entry);

            block_on(futures::future::poll_fn(|cx| {
                return entry.as_mut().poll_elapsed(cx);
            }))
            .unwrap();
        });

        thread::yield_now();

        // This may or may not return Some (depending on how it races with the
        // thread). If it does return None, however, the timer should complete
        // synchronously.
        handle.process_at_time(time_source.now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn drop_timer() {
    model(|| {
        let clock = crate::time::clock::Clock::new();
        let time_source = super::ClockTime::new(clock.clone());

        let inner = super::Inner::new(time_source.clone(), TimeUnpark::mock());
        let handle = InternalHandle::new(Arc::new(Mutex::new(inner)));

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new(&handle_, clock.now() + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));
        });

        thread::yield_now();

        // advance 2s in the future.
        handle.process_at_time(time_source.now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn change_waker() {
    model(|| {
        let clock = crate::time::clock::Clock::new();
        let time_source = super::ClockTime::new(clock.clone());

        let inner = super::Inner::new(time_source.clone(), TimeUnpark::mock());
        let handle = InternalHandle::new(Arc::new(Mutex::new(inner)));

        let handle_ = handle.clone();
        let jh = thread::spawn(move || {
            let entry = TimerEntry::new(&handle_, clock.now() + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

            block_on(futures::future::poll_fn(|cx| {
                return entry.as_mut().poll_elapsed(cx);
            }))
            .unwrap();
        });

        thread::yield_now();

        // advance 2s
        handle.process_at_time(time_source.now() + 2_000_000_000);

        jh.join().unwrap();
    })
}

#[test]
fn reset_future() {
    model(|| {
        let finished_early = Arc::new(AtomicBool::new(false));

        let clock = crate::time::clock::Clock::new();
        let time_source = super::ClockTime::new(clock.clone());

        let inner = super::Inner::new(time_source.clone(), TimeUnpark::mock());
        let handle = InternalHandle::new(Arc::new(Mutex::new(inner)));

        let handle_ = handle.clone();
        let finished_early_ = finished_early.clone();
        let start = clock.now();

        let jh = thread::spawn(move || {
            let entry = TimerEntry::new(&handle_, start + Duration::from_secs(1));
            pin!(entry);

            let _ = entry
                .as_mut()
                .poll_elapsed(&mut Context::from_waker(futures::task::noop_waker_ref()));

            entry.as_mut().reset(start + Duration::from_secs(2));

            // shouldn't complete before 2s
            block_on(futures::future::poll_fn(|cx| {
                return entry.as_mut().poll_elapsed(cx);
            }))
            .unwrap();

            finished_early_.store(true, Ordering::Relaxed);
        });

        thread::yield_now();

        // This may or may not return a wakeup time.
        handle.process_at_time(time_source.instant_to_tick(start + Duration::from_millis(1500)));

        assert!(!finished_early.load(Ordering::Relaxed));

        handle.process_at_time(time_source.instant_to_tick(start + Duration::from_millis(2500)));

        jh.join().unwrap();

        assert!(finished_early.load(Ordering::Relaxed));
    })
}

/*
#[test]
fn balanced_incr_and_decr() {
    const OPS: usize = 5;

    fn incr(inner: Arc<Inner>) {
        for _ in 0..OPS {
            inner.increment().expect("increment should not have failed");
            thread::yield_now();
        }
    }

    fn decr(inner: Arc<Inner>) {
        let mut ops_performed = 0;
        while ops_performed < OPS {
            if inner.num(Ordering::Relaxed) > 0 {
                ops_performed += 1;
                inner.decrement();
            }
            thread::yield_now();
        }
    }

    loom::model(|| {
        let unpark = Box::new(MockUnpark);
        let instant = Instant::now();

        let inner = Arc::new(Inner::new(instant, unpark));

        let incr_inner = inner.clone();
        let decr_inner = inner.clone();

        let incr_hndle = thread::spawn(move || incr(incr_inner));
        let decr_hndle = thread::spawn(move || decr(decr_inner));

        incr_hndle.join().expect("should never fail");
        decr_hndle.join().expect("should never fail");

        assert_eq!(inner.num(Ordering::SeqCst), 0);
    })
}
*/
