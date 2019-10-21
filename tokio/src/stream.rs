//! A sequence of asynchronous values.

#[cfg(feature = "timer")]
use std::time::Duration;

#[cfg(feature = "timer")]
use crate::timer::{throttle::Throttle, Timeout};

#[doc(inline)]
pub use futures_core::Stream;
#[doc(inline)]
pub use futures_util::stream::{empty, iter, once, pending, poll_fn, repeat, unfold};

/// An extension trait for `Stream` that provides a variety of convenient
/// combinator functions.
///
/// Currently, there are only [`timeout`] and [`throttle`] functions, but
/// this will increase over time.
///
/// Users are not expected to implement this trait. All types that implement
/// `Stream` already implement `StreamExt`.
///
/// This trait can be imported directly or via the Tokio prelude: `use
/// tokio::prelude::*`.
///
/// [`throttle`]: method.throttle
/// [`timeout`]: method.timeout
pub trait StreamExt: Stream {
    /// Throttle down the stream by enforcing a fixed delay between items.
    ///
    /// Errors are also delayed.
    #[cfg(feature = "timer")]
    fn throttle(self, duration: Duration) -> Throttle<Self>
    where
        Self: Sized,
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
    /// use tokio::prelude::*;
    ///
    /// use std::time::Duration;
    ///
    /// # fn slow_stream() -> impl Stream<Item = ()> {
    /// #   tokio::stream::empty()
    /// # }
    /// #
    /// # async fn dox() {
    /// let mut stream = slow_stream()
    ///     .timeout(Duration::from_secs(1));
    ///
    /// while let Some(value) = stream.next().await {
    ///     println!("value = {:?}", value);
    /// }
    /// # }
    /// ```
    #[cfg(feature = "timer")]
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        Timeout::new(self, timeout)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
