use Error;
use timer::{Handle, Entry};

use futures::Poll;

use std::sync::Arc;
use std::time::Instant;

/// Registration with a timer.
///
/// The association between a `Delay` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub(crate) struct Registration {
    entry: Arc<Entry>,
}

impl Registration {
    pub fn new(deadline: Instant) -> Registration {
        fn is_send<T: Send + Sync>() {}
        is_send::<Registration>();

        match Handle::try_current() {
            Ok(handle) => Registration::new_with_handle(deadline, handle),
            Err(_) => Registration::new_error(),
        }
    }

    pub fn new_with_handle(deadline: Instant, handle: Handle) -> Registration {
        let inner = match handle.inner() {
            Some(inner) => inner,
            None => return Registration::new_error(),
        };

        // Increment the number of active timeouts
        if inner.increment().is_err() {
            return Registration::new_error();
        }

        let when = inner.normalize_deadline(deadline);

        if when <= inner.elapsed() {
            // The deadline has already elapsed, ther eis no point creating the
            // structures.
            return Registration {
                entry: Arc::new(Entry::new_elapsed(handle)),
            };
        }

        let entry = Arc::new(Entry::new(when, handle));

        if inner.queue(&entry).is_err() {
            // The timer has shutdown, transition the entry to the error state.
            entry.error();
        }

        Registration { entry }
    }

    pub fn reset(&self, deadline: Instant) {
        Entry::reset(&self.entry, deadline);
    }

    fn new_error() -> Registration {
        let entry = Arc::new(Entry::new_error());
        Registration { entry }
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
