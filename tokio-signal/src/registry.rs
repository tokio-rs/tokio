use std::ops;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::os::{OsExtraData, OsStorage};
use futures::sync::mpsc::Sender;

pub(crate) type EventId = usize;

/// State for a specific event, whether a notification is pending delivery,
/// and what listeners are registered.
#[derive(Default, Debug)]
pub(crate) struct EventInfo {
    pending: AtomicBool,
    recipients: Mutex<Vec<Sender<()>>>,
}

/// An interface for retrieving the `EventInfo` for a particular eventId.
pub(crate) trait Storage {
    /// Get the `EventInfo` for `id` if it exists.
    fn event_info(&self, id: EventId) -> Option<&EventInfo>;

    /// Invoke `f` once for each defined `EventInfo` in this storage.
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
        self.iter().for_each(f)
    }
}

/// An interface for initializing a type. Useful for situations where we cannot
/// inject a configured instance in the constructor of another type.
pub(crate) trait Init {
    fn init() -> Self;
}

/// Manages and distributes event notifications to any registered listeners.
///
/// Generic over the underlying storage to allow for domain specific
/// optimizations (e.g. eventIds may or may not be contiguous).
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
    /// Register a new listener for `event_id`.
    fn register_listener(&self, event_id: EventId, listener: Sender<()>) {
        self.storage
            .event_info(event_id)
            .unwrap_or_else(|| panic!("invalid event_id: {}", event_id))
            .recipients
            .lock()
            .unwrap()
            .push(listener);
    }

    /// Mark `event_id` as having been delivered, without broadcasting it to
    /// any listeners.
    fn record_event(&self, event_id: EventId) {
        self.storage
            .event_info(event_id)
            .map(|event_info| event_info.pending.store(true, Ordering::SeqCst));
    }

    /// Broadcast all previously recorded events to their respective listeners.
    fn broadcast(&self) {
        self.storage.for_each(|event_info| {
            // Any signal of this kind arrived since we checked last?
            if !event_info.pending.swap(false, Ordering::SeqCst) {
                return;
            }

            let mut recipients = event_info.recipients.lock().unwrap();

            // Notify all waiters on this signal that the signal has been
            // received. If we can't push a message into the queue then we don't
            // worry about it as everything is coalesced anyway. If the channel
            // has gone away then we can remove that slot.
            for i in (0..recipients.len()).rev() {
                match recipients[i].try_send(()) {
                    Ok(()) => {}
                    Err(ref e) if e.is_disconnected() => {
                        recipients.swap_remove(i);
                    }

                    // Channel is full, ignore the error since the
                    // receiver has already been woken up
                    Err(e) => {
                        // Sanity check in case this error type ever gets
                        // additional variants we have not considered.
                        debug_assert!(e.is_full());
                    }
                }
            }
        });
    }
}

pub(crate) struct Globals {
    extra: OsExtraData,
    registry: Registry<OsStorage>,
}

impl ops::Deref for Globals {
    type Target = OsExtraData;

    fn deref(&self) -> &Self::Target {
        &self.extra
    }
}

impl Globals {
    /// Register a new listener for `event_id`.
    pub(crate) fn register_listener(&self, event_id: EventId, listener: Sender<()>) {
        self.registry.register_listener(event_id, listener);
    }

    /// Mark `event_id` as having been delivered, without broadcasting it to
    /// any listeners.
    pub(crate) fn record_event(&self, event_id: EventId) {
        self.registry.record_event(event_id);
    }

    /// Broadcast all previously recorded events to their respective listeners.
    pub(crate) fn broadcast(&self) {
        self.registry.broadcast()
    }

    #[cfg(unix)]
    pub(crate) fn storage(&self) -> &OsStorage {
        &self.registry.storage
    }
}

pub(crate) fn globals() -> Pin<&'static Globals>
where
    OsExtraData: 'static + Send + Sync + Init,
    OsStorage: 'static + Send + Sync + Init,
{
    lazy_static! {
        static ref GLOBALS: Pin<Box<Globals>> = Pin::new(Box::new(Globals {
            extra: OsExtraData::init(),
            registry: Registry::new(OsStorage::init()),
        }));
    }

    GLOBALS.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::sync::mpsc::channel;
    use futures::sync::oneshot;
    use futures::{Future, Stream};

    use std::time::Duration;
    use tokio::runtime::Runtime;
    use tokio_timer::sleep;

    #[test]
    fn smoke() {
        let registry = Registry::new(vec![
            EventInfo::default(),
            EventInfo::default(),
            EventInfo::default(),
        ]);

        let (first_tx, first_rx) = channel(0);
        let (second_tx, second_rx) = channel(0);
        let (third_tx, third_rx) = channel(0);

        registry.register_listener(0, first_tx);
        registry.register_listener(1, second_tx);
        registry.register_listener(2, third_tx);

        let (fire, wait) = oneshot::channel();
        let rt = Runtime::new().unwrap();

        rt.spawn(
            wait.and_then(move |_| {
                // Record some events which should get coalesced
                registry.record_event(0);
                registry.record_event(0);
                registry.record_event(1);
                registry.record_event(1);
                registry.broadcast();

                sleep(Duration::from_millis(100))
                    .map_err(|e| panic!("{:#?}", e))
                    .and_then(move |_| {
                        registry.record_event(0);
                        registry.broadcast();

                        drop(registry);
                        Ok(())
                    })
            })
            .map_err(|e| panic!("{}", e)),
        );

        let (first_results, second_results, third_results) = rt
            .block_on(futures::lazy(move || {
                let _ = fire.send(());

                first_rx
                    .collect()
                    .join3(second_rx.collect(), third_rx.collect())
            }))
            .expect("failed to extract events");

        assert_eq!(2, first_results.len());
        assert_eq!(1, second_results.len());
        assert_eq!(0, third_results.len());
    }

    #[test]
    #[should_panic = "invalid event_id: 1"]
    fn register_panics_on_invalid_input() {
        let registry = Registry::new(vec![EventInfo::default()]);

        let (tx, _) = channel(0);
        registry.register_listener(1, tx);
    }

    #[test]
    fn record_invalid_event_does_nothing() {
        let registry = Registry::new(vec![EventInfo::default()]);
        registry.record_event(42);
    }

    #[test]
    fn broadcast_cleans_up_disconnected_listeners() {
        let registry = Registry::new(vec![EventInfo::default()]);

        let (first_tx, first_rx) = channel(0);
        let (second_tx, second_rx) = channel(0);
        let (third_tx, third_rx) = channel(0);

        registry.register_listener(0, first_tx);
        registry.register_listener(0, second_tx);
        registry.register_listener(0, third_tx);

        drop(first_rx);
        drop(second_rx);

        let (fire, wait) = oneshot::channel();
        let rt = Runtime::new().unwrap();

        rt.spawn(
            wait.and_then(move |_| {
                // Record some events which should get coalesced
                registry.record_event(0);
                registry.broadcast();

                assert_eq!(1, registry.storage[0].recipients.lock().unwrap().len());
                drop(registry);

                Ok(())
            })
            .map_err(|e| panic!("{}", e)),
        );

        let results = rt
            .block_on(futures::lazy(move || {
                let _ = fire.send(());
                third_rx.collect()
            }))
            .expect("failed to extract events");

        assert_eq!(1, results.len());
    }
}
