use tokio_timer::{DebounceLeading, DebounceTrailing, Timeout};

use futures::Stream;

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
    // TODO: Use parameter instead of two methods for leading / trailing edge?

    /// Debounce the stream discarding items that passed through within a
    /// certain timeframe.
    ///
    /// Errors will pass through without being debounced. Debouncing will happen
    /// on the leading edge. This means the first item will be passed on immediately,
    /// and only then the following ones will be discarded until the specified
    /// duration has elapsed without having seen an item.
    fn debounce_leading(self, dur: Duration) -> DebounceLeading<Self>
    where Self:Sized
    {
        DebounceLeading::new(self, dur)
    }

    /// Debounce a stream discarding items that are passed through within a
    /// certain timeframe.
    ///
    /// Errors will pass through without being debounced. Debouncing will
    /// happen on the trailing edge. This means all items (except the last
    /// one) will be discarded until the delay has elapsed without an item
    /// being passed through. The last item that was passed through will
    /// be returned.
    fn debounce_trailing(self, dur: Duration) -> DebounceTrailing<Self>
    where Self: Sized
    {
        DebounceTrailing::new(self, dur)
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
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where Self: Sized,
    {
        Timeout::new(self, timeout)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
