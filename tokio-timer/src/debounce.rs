//! Debounce streams on either the leading or trailing edge for a certain amount
//! of time.
//!
//! See the [`DebounceLeading`] or [`DebounceTrailing`] documentation for more
//! details.
//!
//! [`DebounceLeading`]: struct.DebounceLeading.html
//! [`DebounceTrailing`]: struct.DebounceTrailing.html

use {clock, Delay, Error};

use futures::{Async, Future, Poll, Stream};
use futures::future::Either;

use std::time::Duration;

/// Debounce a stream discarding items that passed through within a certain
/// timeframe.
///
/// Errors will pass through without being debounced. Debouncing will happen
/// on the leading edge. This means the first item will be passed on immediately,
/// and only then the following ones will be discarded until the specified
/// duration has elapsed without having seen an item.
#[derive(Debug)]
pub struct DebounceLeading<T> {
    delay: Option<Delay>,
    duration: Duration,
    stream: Option<T>,
}

/// Debounce a stream discarding items that are passed through within a certain
/// timeframe.
///
/// Errors will pass through without being debounced. Debouncing will happen
/// on the trailing edge. This means all items (except the last one) will be
/// discarded until the delay has elapsed without an item being passed through.
/// The last item that was passed through will be returned.
#[derive(Debug)]
pub struct DebounceTrailing<T: Stream> {
    delay: Option<Delay>,
    duration: Duration,
    last_item: Option<T::Item>,
    stream: Option<T>,
}

impl<T> DebounceLeading<T> {
    /// Constructs a new stream that debounces on the leading edge.
    pub fn new(stream: T, duration: Duration) -> Self {
        Self {
            delay: None,
            duration: duration,
            stream: Some(stream),
        }
    }
}

impl<T: Stream> DebounceTrailing<T> {
    /// Constructs a new stream that debounces on the trailing edge.
    pub fn new(stream: T, duration: Duration) -> Self {
        Self {
            delay: None,
            duration: duration,
            last_item: None,
            stream: Some(stream),
        }
    }
}

impl<T: Stream> Stream for DebounceLeading<T> {
    type Item = T::Item;
    type Error = Either<T::Error, Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.delay.take() {
            Some(mut d) => match d.poll() {
                // Delay has woken us up and is over, just discard
                // the delay future.
                Ok(Async::Ready(_)) => Ok(Async::NotReady),

                // The stream has woken us up, but we have a delay.
                Ok(Async::NotReady) => match self.stream.take() {
                    Some(mut s) => match s.poll() {
                        // We have gotten an item, but we're currently blocked on
                        // the delay. Discard it and reset the timer.
                        Ok(Async::Ready(Some(_))) => {
                            self.delay = Some(Delay::new(clock::now() + self.duration));
                            self.stream = Some(s);

                            Ok(Async::NotReady)
                        },

                        // The stream has ended. Communicate this immediately to the
                        // following stream.
                        Ok(Async::Ready(None)) => Ok(Async::Ready(None)),

                        Ok(Async::NotReady) => {
                            self.delay = Some(d);
                            self.stream = Some(s);

                            Ok(Async::NotReady)
                        },

                        Err(e) => Err(Either::A(e)),
                    },
                    None => Ok(Async::Ready(None)),
                },
                Err(e) => Err(Either::B(e)),
            },

            None => match self.stream.take() {
                Some(mut s) => match s.poll() {
                    // We have gotten an item. Set up the delay for future items to bep
                    // debounced and return it.
                    Ok(Async::Ready(Some(item))) => {
                        self.delay = Some(Delay::new(clock::now() + self.duration));
                        self.stream = Some(s);

                        Ok(Async::Ready(Some(item)))
                    },

                    // The stream has ended. Communicate this immediately to the
                    // following stream.
                    Ok(Async::Ready(None)) => Ok(Async::Ready(None)),

                    Ok(Async::NotReady) => {
                        self.stream = Some(s);

                        Ok(Async::NotReady)
                    },

                    Err(e) => Err(Either::A(e)),
                },
                None => Ok(Async::Ready(None)),
            }
        }
    }
}

impl<T: Stream> Stream for DebounceTrailing<T> {
    type Item = T::Item;
    type Error = Either<T::Error, Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.delay.take() {
            Some(mut d) => match d.poll() {
                // Delay has woken us up and is over, see if we have an item to return
                Ok(Async::Ready(_)) => Ok(Async::Ready(self.last_item.take())),

                // The stream has woken us up, but we have a delay.
                Ok(Async::NotReady) => match self.stream.take() {
                    Some(mut s) => match s.poll() {
                        // We have gotten an item, but we're currently blocked on
                        // the delay. Save it in the struct for returning it later.
                        Ok(Async::Ready(Some(item))) => {
                            self.delay = Some(Delay::new(clock::now() + self.duration));
                            self.last_item = Some(item);
                            self.stream = Some(s);

                            Ok(Async::NotReady)
                        },

                        // The stream has ended. Communicate this immediately to the
                        // following stream.
                        Ok(Async::Ready(None)) => Ok(Async::Ready(None)),

                        Ok(Async::NotReady) => {
                            self.delay = Some(d);
                            self.stream = Some(s);

                            Ok(Async::NotReady)
                        },

                        Err(e) => Err(Either::A(e)),
                    },
                    None => Ok(Async::Ready(None)),
                },
                Err(e) => Err(Either::B(e)),
            },

            None => match self.stream.take() {
                Some(mut s) => match s.poll() {
                    // We have gotten an item. Set up the delay to return it after and
                    // save it in the struct for possibly returning it later.
                    Ok(Async::Ready(Some(item))) => {
                        self.delay = Some(Delay::new(clock::now() + self.duration));
                        self.last_item = Some(item);
                        self.stream = Some(s);

                        Ok(Async::NotReady)
                    },

                    // The stream has ended. Communicate this immediately to the
                    // following stream.
                    Ok(Async::Ready(None)) => Ok(Async::Ready(None)),

                    Ok(Async::NotReady) => {
                        self.stream = Some(s);

                        Ok(Async::NotReady)
                    },

                    Err(e) => Err(Either::A(e)),
                },
                None => Ok(Async::Ready(None)),
            }
        }
    }
}
