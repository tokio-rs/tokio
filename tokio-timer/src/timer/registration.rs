use crate::timer::{Entry, HandlePriv};
use crate::Error;
use std::sync::Arc;
use std::task::{self, Poll};
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

    // Used by `Timeout<Stream>`
    #[cfg(feature = "timeout-stream")]
    pub fn reset_timeout(&mut self) {
        let deadline = crate::clock::now() + self.entry.time_ref().duration;
        self.entry.time_mut().deadline = deadline;
        Entry::reset(&mut self.entry);
    }

    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    pub fn poll_elapsed(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        self.entry.poll_elapsed(cx)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        Entry::cancel(&self.entry);
    }
}
