//! Docs

use Sleep;

use futures::{Future, Poll, Async};

use std::time::Instant;

/// Allows a given `Future` up to the specified deadline.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Deadline<T> {
    future: T,
    sleep: Sleep,
}

/// Error returned by `Deadline` future.
#[derive(Debug)]
pub struct DeadlineError<T> {
    kind: Kind<T>,
}

/// Deadline error variants
#[derive(Debug)]
enum Kind<T> {
    /// Inner future returned an error
    Inner(T),

    /// The deadline elapsed.
    Elapsed,

    /// Timer returned an error.
    Timer(::Error),
}

impl<T> Deadline<T> {
    pub fn new(future: T, deadline: Instant) -> Deadline<T> {
        Deadline {
            future: future,
            sleep: Sleep::new(deadline),
        }
    }

    /// Gets a reference to the underlying future in this deadline.
    ///
    /// # Return
    ///
    /// The function returns `None` if the inner future has already been
    /// consumed.
    pub fn get_ref(&self) -> &T {
        &self.future
    }

    /// Gets a mutable reference to the underlying future in this deadline.
    ///
    /// # Return
    ///
    /// The function returns `None` if the inner future has already been
    /// consumed.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.future
    }

    /// Consumes this deadline, returning the underlying future.
    ///
    /// # Return
    ///
    /// The function returns `None` if the inner future has already been
    /// consumed.
    pub fn into_inner(self) -> T {
        self.future
    }
}

impl<T> Future for Deadline<T>
where T: Future,
{
    type Item = T::Item;
    type Error = DeadlineError<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.future.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => return Err(DeadlineError::inner(e)),
        }

        // Now check the timer
        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                Err(DeadlineError::elapsed())
            },
            Err(e) => Err(DeadlineError::timer(e)),
        }
    }
}

// ===== impl DeadlineError =====

impl<T> DeadlineError<T> {
    pub fn inner(err: T) -> DeadlineError<T> {
        DeadlineError {
            kind: Kind::Inner(err),
        }
    }

    pub fn is_inner(&self) -> bool {
        match self.kind {
            Kind::Inner(_) => true,
            _ => false,
        }
    }

    pub fn into_inner(self) -> Option<T> {
        match self.kind {
            Kind::Inner(err) => Some(err),
            _ => None,
        }
    }

    pub fn elapsed() -> DeadlineError<T> {
        DeadlineError {
            kind: Kind::Elapsed,
        }
    }

    pub fn is_elapsed(&self) -> bool {
        match self.kind {
            Kind::Elapsed => true,
            _ => false,
        }
    }

    pub fn timer(err: ::Error) -> DeadlineError<T> {
        DeadlineError {
            kind: Kind::Timer(err),
        }
    }

    pub fn is_timer(&self) -> bool {
        match self.kind {
            Kind::Timer(_) => true,
            _ => false,
        }
    }

    pub fn into_timer(self) -> Option<::Error> {
        match self.kind {
            Kind::Timer(err) => Some(err),
            _ => None,
        }
    }
}
