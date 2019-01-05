//! Debounce streams on the leading or trailing edge or both edges for a certain
//! amount of time.
//!
//! See [`Debounce`] for more details.
//!
//! [`Debounce`]: struct.Debounce.html

use {clock, Delay, Error};

use futures::{
    future::Either,
    prelude::*,
};
use std::{
    cmp,
    error::Error as StdError,
    fmt::{Display, Formatter, Result as FmtResult},
    time::{Duration, Instant},
};

/// Debounce streams on the leading or trailing edge or both.
///
/// Useful for slowing processing of e. g. user input or network events
/// to a bearable rate.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Debounce<T: Stream> {
    delay: Option<Delay>,
    duration: Duration,
    edge: Edge,
    last_item: Option<T::Item>,
    max_wait: Option<Duration>,
    max_wait_to: Option<Instant>,
    stream: T,
}

/// Builds a debouncing stream.
#[derive(Debug)]
pub struct DebounceBuilder<T> {
    duration: Option<Duration>,
    edge: Option<Edge>,
    max_wait: Option<Duration>,
    stream: T,
}

/// Either the error of the underlying stream, or an error within tokio's
/// timing machinery.
#[derive(Debug)]
pub struct DebounceError<T>(Either<T, Error>);

/// Which edge the debounce tiggers on.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Edge {
    /// The debounce triggers on the leading edge.
    ///
    /// The first stream item will be returned immediately and subsequent ones
    /// will be ignored until the delay has elapsed without items passing through.
    Leading,

    /// The debounce triggers on the trailing edge.
    ///
    /// All items (except the last one) are thrown away until the delay has elapsed
    /// without items passing through. The last item is returned.
    Trailing,

    /// The debounce triggers on both the leading and the trailing edge.
    ///
    /// The first and the last items will be returned.
    ///
    /// Note that trailing edge behavior will only be visible if the underlying
    /// stream fires at least twice during the debouncing period.
    Both,
}

impl<T: Stream> Debounce<T> {
    /// Constructs a new stream that debounces the items passed through.
    ///
    /// Care must be taken that `stream` returns `Async::NotReady` at some point,
    /// otherwise the debouncing implementation will overflow the stack during
    /// `.poll()` (i. e. don't use this directly on `stream::repeat`).
    pub fn new(
        stream: T,
        duration: Duration,
        edge: Edge,
        max_wait: Option<Duration>,
    ) -> Self {
        Self {
            delay: None,
            duration,
            edge,
            last_item: None,
            max_wait,
            max_wait_to: None,
            stream,
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &T {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this combinator
    /// is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the stream
    /// which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so care
    /// should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> T {
        self.stream
    }

    /// Computes the instant at which the next debounce delay elapses.
    fn delay_time(&mut self) -> Instant {
        let next = clock::now() + self.duration;

        if let Some(to) = self.max_wait_to {
            cmp::min(next, to)
        } else {
            next
        }
    }

    /// Polls the underlying delay future.
    fn poll_delay(d: &mut Delay) -> Poll<(), <Self as Stream>::Error> {
        d.poll().map_err(DebounceError::from_timer_error)
    }

    /// Polls the underlying stream.
    fn poll_stream(
        &mut self,
    ) -> Poll<Option<<Self as Stream>::Item>, <Self as Stream>::Error> {
        self.stream.poll().map_err(DebounceError::from_stream_error)
    }

    /// Starts a new delay using the current duration and maximum waiting time.
    fn start_delay(&mut self) {
        self.max_wait_to = self.max_wait.map(|dur| clock::now() + dur);
        self.delay = Some(Delay::new(self.delay_time()));
    }
}

impl<T: Stream> Stream for Debounce<T> {
    type Item = T::Item;
    type Error = DebounceError<T::Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.delay.take() {
            Some(mut d) => match Self::poll_delay(&mut d)? {
                // Delay has woken us up and is over, if we're trailing edge this
                // means we need to return the last item.
                Async::Ready(_) => {
                    if self.edge.is_trailing() {
                        if let Some(item) = self.last_item.take() {
                            return Ok(Async::Ready(Some(item)));
                        }
                    }

                    return Ok(Async::NotReady);
                },

                // The stream has woken us up, but we have a delay.
                Async::NotReady => match self.poll_stream()? {
                    // We have gotten an item, but we're currently blocked on
                    // the delay. Save it for later and reset the timer.
                    Async::Ready(Some(item)) => {
                        d.reset(self.delay_time());

                        self.delay = Some(d);
                        self.last_item = Some(item);

                        self.poll()
                    },

                    // The stream has ended. Communicate this immediately to the
                    // following stream.
                    Async::Ready(None) => Ok(Async::Ready(None)),

                    Async::NotReady => {
                        self.delay = Some(d);
                        Ok(Async::NotReady)
                    },
                },
            },

            None => match try_ready!(self.poll_stream()) {
                // We have gotten an item. Set up the delay for future items to be
                // debounced. If we're on leading edge, return the item, otherwise
                // save it for later.
                Some(item) => {
                    self.start_delay();

                    if self.edge.is_leading() {
                        Ok(Async::Ready(Some(item)))
                    } else {
                        self.last_item = Some(item);

                        self.poll()
                    }
                },

                // The stream has ended. Communicate this immediately to the
                // following stream.
                None => Ok(Async::Ready(None)),
            },
        }
    }
}

impl<T> DebounceBuilder<T> {
    /// Creates a new builder from the given debounce stream.
    ///
    /// Care must be taken that `stream` returns `Async::NotReady` at some point,
    /// otherwise the debouncing implementation will overflow the stack during
    /// `.poll()` (i. e. don't use this directly on `stream::repeat`).
    pub fn from_stream(stream: T) -> Self {
        DebounceBuilder {
            duration: None,
            edge: None,
            max_wait: None,
            stream: stream,
        }
    }

    /// Sets the duration to debounce to.
    ///
    /// If no duration is set here but [`max_wait`] is given instead, the resulting
    /// stream will sample the underlying stream at the interval given by
    /// [`max_wait`] instead of debouncing it.
    ///
    /// [`max_wait`]: #method.max_wait
    pub fn duration(mut self, dur: Duration) -> Self {
        self.duration = Some(dur);
        self
    }

    /// Sets the debouncing edge.
    ///
    /// An edge MUST be set before trying to [`build`] the debounce stream.
    ///
    /// [`build`]: #method.build
    pub fn edge(mut self, edge: Edge) -> Self {
        self.edge = Some(edge);
        self
    }

    /// Sets the maximum waiting time.
    ///
    /// If only a `max_wait` is given (and no [`duration`]), the resulting stream
    /// will sample the underlying stream at the interval given by `max_wait`
    /// instead of debouncing it.
    /// Sampling cannot occur on both edges. Trying to build a sampling stream
    /// on both edges will panic.
    ///
    /// [`duration`]: #method.duration
    pub fn max_wait(mut self, max_wait: Duration) -> Self {
        self.max_wait = Some(max_wait);
        self
    }
}

impl<T: Stream> DebounceBuilder<T> {
    /// Builds the debouncing stream.
    ///
    /// Panics if the edge or the duration is unspecified, or if only `max_wait`
    /// together with `Edge::Both` was specified.
    pub fn build(self) -> Debounce<T> {
        let edge = self.edge.expect("missing debounce edge");

        // If we've only been given a maximum waiting time, this means we need to
        // sample the stream at the interval given by max_wait instead of
        // debouncing it.
        let duration = match self.max_wait {
            Some(max_wait) => match self.duration {
                Some(dur) => dur,

                None => {
                    // Sampling on both edges leads to unexpected behavior where, when a
                    // sample interval elapses, two items will be returned.
                    assert!(edge != Edge::Both, "cannot sample on both edges");

                    // The actual duration added here doesn't matter, as long as its
                    // means the result is longer than `max_wait` and we have more than
                    // a millisecond (tokio timer precision).
                    max_wait + Duration::from_secs(1)
                },
            },

            None => self.duration.expect("missing debounce duration")
        };

        Debounce::new(
            self.stream,
            duration,
            edge,
            self.max_wait,
        )
    }
}

impl<T> DebounceError<T> {
    /// Creates an error from the given stream error.
    pub fn from_stream_error(err: T) -> Self {
        DebounceError(Either::A(err))
    }

    /// Creates an error from the given timer error.
    pub fn from_timer_error(err: Error) -> Self {
        DebounceError(Either::B(err))
    }

    /// Gets the underlying stream error, if present.
    pub fn get_stream_error(&self) -> Option<&T> {
        match self.0 {
            Either::A(ref err) => Some(err),
            _ => None,
        }
    }

    /// Gets the underlying timer error, if present.
    pub fn get_timer_error(&self) -> Option<&Error> {
        match self.0 {
            Either::B(ref err) => Some(err),
            _ => None,
        }
    }

    /// Attempts to convert the error into the stream error.
    pub fn into_stream_error(self) -> Option<T> {
        match self.0 {
            Either::A(err) => Some(err),
            _ => None,
        }
    }

    /// Attempts to convert the error into the timer error.
    pub fn into_timer_error(self) -> Option<Error> {
        match self.0 {
            Either::B(err) => Some(err),
            _ => None,
        }
    }

    /// Determines whether the underlying error is a stream error.
    pub fn is_stream_error(&self) -> bool {
        !self.is_timer_error()
    }

    /// Determines whether the underlying error is an error within
    /// tokio's timer machinery.
    pub fn is_timer_error(&self) -> bool {
        match self.0 {
            Either::B(_) => true,
            _ => false,
        }
    }
}

impl<T: StdError> Display for DebounceError<T> {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self.0 {
            Either::A(ref err) => write!(f, "stream error: {}", err),
            Either::B(ref err) => write!(f, "timer error: {}", err),
        }
    }
}

impl<T: StdError + 'static> StdError for DebounceError<T> {
    fn description(&self) -> &str {
        match self.0 {
            Either::A(_) => "stream error",
            Either::B(_) => "timer error",
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match self.0 {
            Either::A(ref err) => Some(err),
            Either::B(ref err) => Some(err),
        }
    }
}

impl Edge {
    /// The edge is either leading edge or both edges.
    pub fn is_leading(&self) -> bool {
        match self {
            Edge::Leading | Edge::Both => true,
            _ => false,
        }
    }

    /// The edge is either trailing edge or both edges.
    pub fn is_trailing(&self) -> bool {
        match self {
            Edge::Trailing | Edge::Both => true,
            _ => false
        }
    }
}
