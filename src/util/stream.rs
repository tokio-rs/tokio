#[cfg(feature = "timer")]
use tokio_timer::{
    debounce::{Debounce, DebounceBuilder, Edge},
    throttle::Throttle,
    timeout::Timeout,
};

use futures::Stream;

#[cfg(feature = "timer")]
use std::time::Duration;


/// An extension trait for `Stream` that provides a variety of convenient
/// combinator functions.
///
/// Currently, there only is a [`timeout`] function, but this will increase
/// over time.
///
/// Users are not expected to implement this trait. All types that implement
/// `Stream` already implement `StreamExt`.
///
/// This trait can be imported directly or via the Tokio prelude: `use
/// tokio::prelude::*`.
///
/// [`timeout`]: #method.timeout
pub trait StreamExt: Stream {
    /// Debounce the stream on the trailing edge using the given duration.
    ///
    /// Errors will pass through without being debounced. Debouncing will
    /// happen on the trailing edge. This means all items (except the last
    /// one) will be discarded until the delay has elapsed without an item
    /// being passed through. The last item that was passed through will
    /// be returned.
    ///
    /// Care must be taken that this stream returns `Async::NotReady` at some point,
    /// otherwise the debouncing implementation will overflow the stack during
    /// `.poll()` (i. e. don't use this directly on `stream::repeat`).
    ///
    /// See also [`debounce_builder`], which allows more configuration over how the
    /// debouncing is done.
    ///
    /// [`debounce_builder`]: #method.debounce_builder
    fn debounce(self, dur: Duration) -> Debounce<Self>
    where Self: Sized
    {
        self.debounce_builder()
            .duration(dur)
            .edge(Edge::Trailing)
            .build()
    }

    /// Create a builder that builds a debounced version of this stream.
    ///
    /// The returned builder can be used to configure the debouncing process.
    ///
    /// Care must be taken that this stream returns `Async::NotReady` at some point,
    /// otherwise the debouncing implementation will overflow the stack during
    /// `.poll()` (i. e. don't use this directly on `stream::repeat`).
    fn debounce_builder(self) -> DebounceBuilder<Self>
    where Self: Sized
    {
        DebounceBuilder::from_stream(self)
    }

    /// Sample the stream at the given `interval`.
    ///
    /// Sampling works similar to debouncing in that frequent values will be
    /// ignored. Sampling, however, ensures that an item is passed through at
    /// least after every `interval`. Debounce, on the other hand, would not
    /// pass items through until there has been enough "silence".
    ///
    /// Care must be taken that this stream returns `Async::NotReady` at some point,
    /// otherwise the sampling implementation will overflow the stack during
    /// `.poll()` (i. e. don't use this directly on `stream::repeat`).
    fn sample(self, interval: Duration) -> Debounce<Self>
    where Self: Sized
    {
        self.debounce_builder()
            .max_wait(interval)
            .edge(Edge::Leading)
            .build()
    }

    /// Throttle down the stream by enforcing a fixed delay between items.
    ///
    /// Errors are also delayed.
    #[cfg(feature = "timer")]
    fn throttle(self, duration: Duration) -> Throttle<Self>
    where Self: Sized
    {
        Throttle::new(self, duration)
    }

    /// Creates a new stream which allows `self` until `timeout`.
    ///
    /// This combinator creates a new stream which wraps the receiving stream
    /// with a timeout. For each item, the returned stream is allowed to execute
    /// until it completes or `timeout` has elapsed, whichever happens first.
    ///
    /// If an item completes before `timeout` then the stream will yield
    /// with that item. Otherwise the stream will yield to an error.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
    /// use tokio::prelude::*;
    /// use std::time::Duration;
    /// # use futures::future::{self, FutureResult};
    ///
    /// # fn long_future() -> FutureResult<(), ()> {
    /// #   future::ok(())
    /// # }
    /// #
    /// # fn main() {
    /// let stream = long_future()
    ///     .into_stream()
    ///     .timeout(Duration::from_secs(1))
    ///     .for_each(|i| future::ok(println!("item = {:?}", i)))
    ///     .map_err(|e| println!("error = {:?}", e));
    ///
    /// tokio::run(stream);
    /// # }
    /// ```
    #[cfg(feature = "timer")]
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where Self: Sized,
    {
        Timeout::new(self, timeout)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
