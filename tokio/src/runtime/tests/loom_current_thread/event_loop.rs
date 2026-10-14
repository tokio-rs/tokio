//! The host notification protocol of an event loop: a remote wakeup that
//! arrives around a tick is never lost. Either the host's callback fires
//! for it, or the work it pushed is there for `next_sleep` after the tick.

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::{Arc, Mutex};
use crate::loom::thread;
use crate::runtime::event_loop::{Signal, SignalCfg};
use crate::runtime::Builder;

use std::sync::atomic::Ordering::{Acquire, Release, SeqCst};
use std::time::Duration;

/// The production protocol end to end: the runtime's inject queue, `tick`
/// and `next_sleep`, with spawns from another thread around the tick.
#[test]
fn spawns_around_tick_are_never_lost() {
    loom::model(|| {
        let notified = Arc::new(AtomicBool::new(false));
        let ran = Arc::new(AtomicUsize::new(0));
        let mut event_loop = Builder::new_current_thread()
            .on_work_available({
                let notified = notified.clone();
                move || notified.store(true, SeqCst)
            })
            .build_event_loop()
            .unwrap();

        let spawner = {
            let handle = event_loop.handle().clone();
            let ran = ran.clone();
            thread::spawn(move || {
                for _ in 0..2 {
                    let ran = ran.clone();
                    handle.spawn(async move {
                        ran.fetch_add(1, SeqCst);
                    });
                }
            })
        };

        event_loop.tick(usize::MAX);
        let reported = event_loop.next_sleep() == Some(Duration::ZERO);

        spawner.join().unwrap();
        assert!(
            ran.load(SeqCst) == 2 || reported || notified.load(SeqCst),
            "a spawn neither run by the tick, nor reported after it, nor notified"
        );
        event_loop.shutdown_background();
    });
}

#[test]
fn wake_around_tick_is_never_lost() {
    loom::model(|| {
        let notified = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(signal(notified.clone()));
        let inject = Arc::new(Inject::default());

        let waker = {
            let (signal, inject) = (signal.clone(), inject.clone());
            thread::spawn(move || {
                inject.push();
                signal.wake();
            })
        };

        signal.begin();
        let drained = inject.drain();
        signal.end();
        let reported = !inject.is_empty();

        waker.join().unwrap();
        assert!(
            drained == 1 || reported || notified.load(SeqCst),
            "a wakeup neither consumed by the tick, nor reported after it, nor notified"
        );
    });
}

/// A wakeup coalesced into a tick must not leave `pending` set past the
/// tick's end, or the next wakeup is suppressed while the host sleeps.
#[test]
fn wake_after_tick_is_never_lost() {
    loom::model(|| {
        let notified = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(signal(notified.clone()));
        let inject = Arc::new(Inject::default());

        let waker = {
            let (signal, inject) = (signal.clone(), inject.clone());
            thread::spawn(move || {
                inject.push();
                signal.wake();
                inject.push();
                signal.wake();
            })
        };

        signal.begin();
        inject.drain();
        signal.end();
        let reported = !inject.is_empty();

        waker.join().unwrap();
        assert!(
            inject.drain() == 0 || reported || notified.load(SeqCst),
            "a wakeup neither consumed by the tick, nor reported after it, nor notified"
        );
    });
}

/// An `Unparker::unpark` carries no work for `next_sleep` to report, so it
/// must notify whatever the tick is doing.
#[test]
fn unpark_around_tick_notifies() {
    loom::model(|| {
        let notified = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(signal(notified.clone()));

        let unparker = {
            let signal = signal.clone();
            thread::spawn(move || signal.unpark())
        };

        signal.begin();
        signal.end();

        unparker.join().unwrap();
        assert!(notified.load(SeqCst));
    });
}

/// The inject queue's synchronization: `len` is written under the lock
/// with Release and read by `next_sleep` without it.
struct Inject {
    synced: Mutex<usize>,
    len: AtomicUsize,
}

impl Default for Inject {
    fn default() -> Inject {
        Inject {
            synced: Mutex::new(0),
            len: AtomicUsize::new(0),
        }
    }
}

impl Inject {
    fn push(&self) {
        let mut synced = self.synced.lock();
        *synced += 1;
        self.len.store(*synced, Release);
    }

    fn drain(&self) -> usize {
        let mut synced = self.synced.lock();
        self.len.store(0, Release);
        std::mem::take(&mut *synced)
    }

    fn is_empty(&self) -> bool {
        self.len.load(Acquire) == 0
    }
}

fn signal(notified: Arc<AtomicBool>) -> Signal {
    Signal::new(
        SignalCfg {
            on_work_available: Some(std::sync::Arc::new(move || notified.store(true, SeqCst))),
            #[cfg(unix)]
            event_fd: false,
        },
        false,
    )
    .unwrap()
}
