#[cfg(feature = "timer")]
use tokio_timer::{throttle::Throttle, Timeout};

use futures::Stream;

#[cfg(feature = "timer")]
use std::time::Duration;
pub use util::enumerate::Enumerate;

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
/// [`timeout`]: #method.timeout
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

    /// Creates a new stream which gives the current iteration count as well
    /// as the next value.
    ///
    /// The stream returned yields pairs `(i, val)`, where `i` is the
    /// current index of iteration and `val` is the value returned by the
    /// iterator.
    ///
    /// # Overflow Behavior
    ///
    /// The method does no guarding against overflows, so counting elements of
    /// an iterator with more than [`std::usize::MAX`] elements either produces the
    /// wrong result or panics.
    fn enumerate(self) -> Enumerate<Self>
    where
        Self: Sized,
    {
        Enumerate::new(self)
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
    where
        Self: Sized,
    {
        Timeout::new(self, timeout)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
