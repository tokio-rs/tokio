use crate::signal::unix::{OsExtraData, OsStorage};
use crate::sync::watch;

use std::ops;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub(crate) type EventId = usize;

/// State for a specific event, whether a notification is pending delivery,
/// and what listeners are registered.
#[derive(Debug)]
pub(crate) struct EventInfo {
    pending: AtomicBool,
    tx: watch::Sender<()>,
}

impl Default for EventInfo {
    fn default() -> Self {
        let (tx, _rx) = watch::channel(());

        Self {
            pending: AtomicBool::new(false),
            tx,
        }
    }
}

/// An interface for retrieving the `EventInfo` for a particular `eventId`.
pub(crate) trait Storage {
    /// Gets the `EventInfo` for `id` if it exists.
    fn event_info(&self, id: EventId) -> Option<&EventInfo>;

    /// Invokes `f` once for each defined `EventInfo` in this storage.
    fn for_each<'a, F>(&'a self, f: F)
    where
        F: FnMut(&'a EventInfo);
}

impl Storage for Vec<EventInfo> {
    fn event_info(&self, id: EventId) -> Option<&EventInfo> {
        self.get(id)
    }

    fn for_each<'a, F>(&'a self, f: F)
    where
        F: FnMut(&'a EventInfo),
    {
        self.iter().for_each(f);
    }
}

/// Manages and distributes event notifications to any registered listeners.
///
/// Generic over the underlying storage to allow for domain specific
/// optimizations (e.g. `eventIds` may or may not be contiguous).
#[derive(Debug)]
pub(crate) struct Registry<S> {
    storage: S,
}

impl<S> Registry<S> {
    fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S: Storage> Registry<S> {
    /// Registers a new listener for `event_id`.
    fn register_listener(&self, event_id: EventId) -> watch::Receiver<()> {
        self.storage
            .event_info(event_id)
            .unwrap_or_else(|| panic!("invalid event_id: {event_id}"))
            .tx
            .subscribe()
    }

    /// Marks `event_id` as having been delivered, without broadcasting it to
    /// any listeners.
    fn record_event(&self, event_id: EventId) {
        if let Some(event_info) = self.storage.event_info(event_id) {
            event_info.pending.store(true, Ordering::SeqCst);
        }
    }

    /// Discards all recorded events without broadcasting them.
    fn clear_pending(&self) {
        self.storage.for_each(|event_info| {
            event_info.pending.store(false, Ordering::SeqCst);
        });
    }

    /// Broadcasts all previously recorded events to their respective listeners.
    ///
    /// Returns `true` if an event was delivered to at least one listener.
    fn broadcast(&self) -> bool {
        let mut did_notify = false;
        self.storage.for_each(|event_info| {
            // Any signal of this kind arrived since we checked last?
            if !event_info.pending.swap(false, Ordering::SeqCst) {
                return;
            }

            // Ignore errors if there are no listeners
            if event_info.tx.send(()).is_ok() {
                did_notify = true;
            }
        });

        did_notify
    }
}

pub(crate) struct Globals {
    extra: OsExtraData,
    registry: Registry<OsStorage>,
    /// `process::id()` of the process whose self-pipe is currently installed
    /// in `extra`. A forked child inherits its parent's pipe and must replace
    /// it before use, see [`Globals::reinit_after_fork_if_needed`].
    ///
    /// Known limitation: if the process recorded here dies and a descendant
    /// that never created a runtime forks a child that is assigned the
    /// recycled PID, the fork goes undetected. This is inherent to PID
    /// comparison; closing it would require `pthread_atfork`, a process-wide
    /// hook that cannot be removed again and is deliberately avoided here.
    pid: Mutex<u32>,
}

impl ops::Deref for Globals {
    type Target = OsExtraData;

    fn deref(&self) -> &Self::Target {
        &self.extra
    }
}

impl Globals {
    /// Registers a new listener for `event_id`.
    pub(crate) fn register_listener(&self, event_id: EventId) -> watch::Receiver<()> {
        self.registry.register_listener(event_id)
    }

    /// Marks `event_id` as having been delivered, without broadcasting it to
    /// any listeners.
    pub(crate) fn record_event(&self, event_id: EventId) {
        self.registry.record_event(event_id);
    }

    /// Broadcasts all previously recorded events to their respective listeners.
    ///
    /// Returns `true` if an event was delivered to at least one listener.
    pub(crate) fn broadcast(&self) -> bool {
        self.registry.broadcast()
    }

    #[cfg(unix)]
    pub(crate) fn storage(&self) -> &OsStorage {
        &self.registry.storage
    }

    /// Re-creates the process-wide self-pipe if this process was forked from
    /// the process that initialized it, so that signal wakeups are no longer
    /// shared with the parent process. See [`OsExtraData::replace_pipe`] for
    /// why the pipe must be replaced and how the replacement keeps the
    /// inherited signal handler working.
    ///
    /// This runs once per runtime creation (a cold path), so a plain mutex
    /// around the PID is plenty.
    pub(crate) fn reinit_after_fork_if_needed(&self) -> std::io::Result<()> {
        let mut pid = self.pid.lock().unwrap();
        let current = std::process::id();
        if *pid == current {
            return Ok(());
        }

        // Invariant: no signal listener can exist in this process yet —
        // registering one requires a signal driver, and the first driver of
        // this process is the caller, which is still being constructed. So
        // a signal arriving while this runs is recorded with no one to
        // observe it, and discarding it below is indistinguishable from the
        // (unsupported) delivery of a signal before any listener exists.
        self.extra.replace_pipe()?;
        // Events recorded before the fork were meant for the parent process.
        self.registry.clear_pending();
        *pid = current;
        Ok(())
    }
}

fn globals_init() -> Globals
where
    OsExtraData: 'static + Send + Sync + Default,
    OsStorage: 'static + Send + Sync + Default,
{
    Globals {
        extra: OsExtraData::default(),
        registry: Registry::new(OsStorage::default()),
        pid: Mutex::new(std::process::id()),
    }
}

pub(crate) fn globals() -> &'static Globals
where
    OsExtraData: 'static + Send + Sync + Default,
    OsStorage: 'static + Send + Sync + Default,
{
    static GLOBALS: OnceLock<Globals> = OnceLock::new();

    GLOBALS.get_or_init(globals_init)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::runtime::{self, Runtime};
    use crate::sync::{oneshot, watch};

    use futures::future;

    #[test]
    fn smoke() {
        let rt = rt();
        rt.block_on(async move {
            let registry = Registry::new(vec![
                EventInfo::default(),
                EventInfo::default(),
                EventInfo::default(),
            ]);

            let first = registry.register_listener(0);
            let second = registry.register_listener(1);
            let third = registry.register_listener(2);

            let (fire, wait) = oneshot::channel();

            crate::spawn(async {
                wait.await.expect("wait failed");

                // Record some events which should get coalesced
                registry.record_event(0);
                registry.record_event(0);
                registry.record_event(1);
                registry.record_event(1);
                registry.broadcast();

                // Yield so the previous broadcast can get received
                //
                // This yields many times since the block_on task is only polled every 61
                // ticks.
                for _ in 0..100 {
                    crate::task::yield_now().await;
                }

                // Send subsequent signal
                registry.record_event(0);
                registry.broadcast();

                drop(registry);
            });

            let _ = fire.send(());
            let all = future::join3(collect(first), collect(second), collect(third));

            let (first_results, second_results, third_results) = all.await;
            assert_eq!(2, first_results.len());
            assert_eq!(1, second_results.len());
            assert_eq!(0, third_results.len());
        });
    }

    #[test]
    #[should_panic = "invalid event_id: 1"]
    fn register_panics_on_invalid_input() {
        let registry = Registry::new(vec![EventInfo::default()]);

        registry.register_listener(1);
    }

    #[test]
    fn record_invalid_event_does_nothing() {
        let registry = Registry::new(vec![EventInfo::default()]);
        registry.record_event(1302);
    }

    #[test]
    fn broadcast_returns_if_at_least_one_event_fired() {
        let registry = Registry::new(vec![EventInfo::default(), EventInfo::default()]);

        registry.record_event(0);
        assert!(!registry.broadcast());

        let first = registry.register_listener(0);
        let second = registry.register_listener(1);

        registry.record_event(0);
        assert!(registry.broadcast());

        drop(first);
        registry.record_event(0);
        assert!(!registry.broadcast());

        drop(second);
    }

    fn rt() -> Runtime {
        runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    async fn collect(mut rx: watch::Receiver<()>) -> Vec<()> {
        let mut ret = vec![];

        while let Ok(v) = rx.changed().await {
            ret.push(v);
        }

        ret
    }
}
