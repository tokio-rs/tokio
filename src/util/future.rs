#[allow(deprecated)]
use tokio_timer::Deadline;
use tokio_timer::Timeout;

use futures::Future;

use std::time::{Instant, Duration};


/// An extension trait for `Future` that provides a variety of convenient
/// combinator functions.
///
/// Currently, there only is a [`timeout`] function, but this will increase
/// over time.
///
/// Users are not expected to implement this trait. All types that implement
/// `Future` already implement `FutureExt`.
///
/// This trait can be imported directly or via the Tokio prelude: `use
/// tokio::prelude::*`.
///
/// [`timeout`]: #method.timeout
pub trait FutureExt: Future {

    /// Creates a new future which allows `self` until `timeout`.
    ///
    /// This combinator creates a new future which wraps the receiving future
    /// with a timeout. The returned future is allowed to execute until it
    /// completes or `timeout` has elapsed, whichever happens first.
    ///
    /// If the future completes before `timeout` then the future will resolve
    /// with that item. Otherwise the future will resolve to an error.
    ///
    /// The future is guaranteed to be polled at least once, even if `timeout`
    /// is set to zero.
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
    /// let future = long_future()
    ///     .timeout(Duration::from_secs(1))
    ///     .map_err(|e| println!("error = {:?}", e));
    ///
    /// tokio::run(future);
    /// # }
    /// ```
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where Self: Sized,
    {
        Timeout::new(self, timeout)
    }

    #[deprecated(since = "0.1.8", note = "use `timeout` instead")]
    #[allow(deprecated)]
    #[doc(hidden)]
    fn deadline(self, deadline: Instant) -> Deadline<Self>
    where Self: Sized,
    {
        Deadline::new(self, deadline)
    }
}

impl<T: ?Sized> FutureExt for T where T: Future {}

#[cfg(test)]
mod test {
    use super::*;
    use prelude::future;

    #[test]
    fn timeout_polls_at_least_once() {
        let base_future = future::result::<(), ()>(Ok(()));
        let timeouted_future = base_future.timeout(Duration::new(0, 0));
        assert!(timeouted_future.wait().is_ok());
    }
}
