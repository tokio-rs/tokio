//! The host notification protocol of an event loop: a remote wakeup that
//! arrives around a tick is never lost. Either the tick runs its work, the
//! host's callback fires for it, or `next_sleep` reports it after the tick.

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::Arc;
use crate::loom::thread;
use crate::runtime::Builder;

use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

/// Spawns from another thread around a tick. The second may land in the
/// idle period the first one's tick ends into.
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

        // The tick answers every notification so far.
        notified.store(false, SeqCst);
        event_loop.tick(usize::MAX);
        let reported = event_loop.next_sleep() == Some(Duration::ZERO);

        spawner.join().unwrap();
        assert!(
            ran.load(SeqCst) == 2 || reported || notified.load(SeqCst),
            "a spawn neither run by the tick, nor reported after it, nor notified"
        );
    });
}

/// An `Unparker::unpark` around a park ends it.
#[test]
fn unpark_around_park_returns() {
    loom::model(|| {
        let mut event_loop = Builder::new_current_thread().build_event_loop().unwrap();

        let unparker = {
            let unparker = event_loop.unparker();
            thread::spawn(move || unparker.unpark())
        };

        event_loop.park(None);

        unparker.join().unwrap();
    });
}

/// A spawn from another thread around a park ends it.
#[test]
fn spawn_around_park_returns() {
    loom::model(|| {
        let mut event_loop = Builder::new_current_thread().build_event_loop().unwrap();

        let spawner = {
            let handle = event_loop.handle().clone();
            thread::spawn(move || {
                handle.spawn(async {});
            })
        };

        event_loop.park(None);
        assert_eq!(event_loop.next_sleep(), Some(Duration::ZERO));

        spawner.join().unwrap();
    });
}
