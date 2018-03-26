use Error;
use timer::{Handle, Entry};

use futures::Poll;

use std::sync::Arc;
use std::time::{Instant, Duration};

/// Registration with a timer.
///
/// The association between a `Sleep` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub(crate) struct Registration {
    entry: Option<Arc<Entry>>,
}

impl Registration {
    pub fn new(deadline: Instant) -> Registration {
        match Handle::try_current() {
            Ok(handle) => Registration::new_with_handle(deadline, handle),
            Err(_) => Registration::new_error(deadline),
        }
    }

    pub fn new_with_handle(deadline: Instant, handle: Handle) -> Registration {
        let inner = match handle.inner() {
            Some(inner) => inner,
            None => return Registration::new_error(deadline),
        };

        // Always add a ms to the deadline so that it will never expire
        // *beforee* the set deadline. The timer wheel will always round down to
        // the nearest milli (as sub ms values are discarded).
        let deadline = deadline + Duration::from_millis(1);

        if deadline <= inner.now() {
            // The deadline has already elapsed, ther eis no point creating the
            // structures.
            return Registration {
                entry: None
            };
        }

        // Increment the number of active timeouts
        if inner.increment().is_err() {
            return Registration::new_error(deadline);
        }

        let entry = Arc::new(Entry::new(deadline, handle));

        if inner.queue(&entry).is_err() {
            // The timer has shutdown, transition the entry to the error state.
            entry.error();
        }

        Registration { entry: Some(entry) }
    }

    fn new_error(deadline: Instant) -> Registration {
        let entry = Some(Arc::new(Entry::new_error(deadline)));
        Registration { entry }
    }

    pub fn is_elapsed(&self) -> bool {
        self.entry.as_ref()
            .map(|e| e.is_elapsed())
            .unwrap_or(true)
    }

    pub fn poll_elapsed(&self) -> Poll<(), Error> {
        self.entry.as_ref()
            .map(|e| e.poll_elapsed())
            .unwrap_or(Ok(().into()))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let entry = match self.entry {
            Some(ref e) => e,
            None => return,
        };

        Entry::cancel(&entry);
    }
}
