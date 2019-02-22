use clock::now;
use timer::{Entry, HandlePriv};
use Error;

use futures::Poll;

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Registration with a timer.
///
/// The association between a `Delay` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub(crate) struct Registration {
    entry: Arc<Entry>,
}

impl Registration {
    pub fn new(deadline: Instant, duration: Duration) -> Registration {
        fn is_send<T: Send + Sync>() {}
        is_send::<Registration>();

        Registration {
            entry: Arc::new(Entry::new(deadline, duration)),
        }
    }

    pub fn deadline(&self) -> Instant {
        self.entry.time_ref().deadline
    }

    pub fn register(&mut self) {
        if !self.entry.is_registered() {
            Entry::register(&mut self.entry)
        }
    }

    pub fn register_with(&mut self, handle: HandlePriv) {
        Entry::register_with(&mut self.entry, handle)
    }

    pub fn reset(&mut self, deadline: Instant) {
        self.entry.time_mut().deadline = deadline;
        Entry::reset(&mut self.entry);
    }

    pub fn reset_timeout(&mut self) {
        let deadline = now() + self.entry.time_ref().duration;
        self.entry.time_mut().deadline = deadline;
        Entry::reset(&mut self.entry);
    }

    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    pub fn poll_elapsed(&self) -> Poll<(), Error> {
        self.entry.poll_elapsed()
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        Entry::cancel(&self.entry);
    }
}
