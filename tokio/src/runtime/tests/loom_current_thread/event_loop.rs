//! The host notification protocol of an event loop: a remote wakeup that
//! arrives around a tick is never lost. Either the host's callback fires
//! for it, or the work it pushed is there for `next_sleep` after the tick.

use crate::loom::sync::atomic::{AtomicBool, Ordering::SeqCst};
use crate::loom::sync::{Arc, Mutex};
use crate::loom::thread;
use crate::runtime::event_loop::{Signal, SignalCfg};

#[test]
fn wake_around_tick_is_never_lost() {
    loom::model(|| {
        let notified = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(signal(notified.clone()));
        // The inject queue: pushed before the wake, drained by the tick,
        // checked by `next_sleep` after it.
        let inject = Arc::new(Mutex::new(0usize));

        let waker = {
            let (signal, inject) = (signal.clone(), inject.clone());
            thread::spawn(move || {
                *inject.lock() += 1;
                signal.wake();
            })
        };

        signal.begin();
        let drained = std::mem::take(&mut *inject.lock());
        signal.end();
        let remaining = *inject.lock();

        waker.join().unwrap();
        assert!(
            drained == 1 || remaining == 1 || notified.load(SeqCst),
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
