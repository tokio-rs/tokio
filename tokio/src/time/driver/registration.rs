use crate::time::driver::Entry;
use crate::time::{Duration, Error, Instant};

use std::sync::Arc;
use std::task::{self, Poll};

/// Registration with a timer.
///
/// The association between a `Delay` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub(crate) struct Registration {
    entry: Arc<Entry>,
}

impl Registration {
    pub(crate) fn new(deadline: Instant, duration: Duration) -> Registration {
        fn is_send<T: Send + Sync>() {}
        is_send::<Registration>();

        Registration {
            entry: Arc::new(Entry::new(deadline, duration)),
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.entry.time_ref().deadline
    }

    pub(crate) fn register(&mut self) {
        if !self.entry.is_registered() {
            Entry::register(&mut self.entry)
        }
    }

    pub(crate) fn reset(&mut self, deadline: Instant) {
        unsafe {
            self.entry.time_mut().deadline = deadline;
        }

        Entry::reset(&mut self.entry);
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    pub(crate) fn poll_elapsed(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        self.entry.poll_elapsed(cx)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        Entry::cancel(&self.entry);
    }
}
