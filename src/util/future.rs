use tokio_timer::Deadline;

use futures::Future;

use std::time::Instant;


/// An extension trait for `Future` that provides a variety of convenient
/// combinator functions.
///
/// Currently, there only is a [`deadline`] function, but this will increase
/// over time.
///
/// Users are not expected to implement this trait. All types that implement
/// `Future` already implement `FutureExt`.
///
/// This trait can be imported directly or via the Tokio prelude: `use
/// tokio::prelude::*`.
///
/// [`deadline`]: #method.deadline
pub trait FutureExt: Future {

    /// Creates a new future which allows `self` until `deadline`.
    ///
    /// This combinator creates a new future which wraps the receiving future
    /// with a deadline. The returned future is allowed to execute until it
    /// completes or `deadline` is reached, whicheever happens first.
    ///
    /// If the future completes before `deadline` then the future will resolve
    /// with that item. Otherwise the future will resolve to an error once
    /// `deadline` is reached.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
    /// use tokio::prelude::*;
    /// use std::time::{Duration, Instant};
    /// # use futures::future::{self, FutureResult};
    ///
    /// # fn long_future() -> FutureResult<(), ()> {
    /// #   future::ok(())
    /// # }
    /// #
    /// # fn main() {
    /// let future = long_future()
    ///     .deadline(Instant::now() + Duration::from_secs(1))
    ///     .map_err(|e| println!("error = {:?}", e));
    ///
    /// tokio::run(future);
    /// # }
    /// ```
    fn deadline(self, deadline: Instant) -> Deadline<Self>
    where Self: Sized,
    {
        Deadline::new(self, deadline)
    }
}

impl<T: ?Sized> FutureExt for T where T: Future {}
