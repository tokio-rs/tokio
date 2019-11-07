//! Asynchronous values.

#[cfg(feature = "time")]
use crate::time::Timeout;

#[cfg(feature = "time")]
use std::time::Duration;

#[doc(inline)]
pub use futures_util::future::{err, ok, pending, poll_fn, ready};
#[doc(inline)]
pub use std::future::Future;

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
    /// use tokio::prelude::*;
    /// use std::time::Duration;
    ///
    /// async fn long_future() {
    ///     // do work here
    /// }
    ///
    /// # async fn dox() {
    /// let res = long_future()
    ///     .timeout(Duration::from_secs(1))
    ///     .await;
    ///
    /// if res.is_err() {
    ///     println!("operation timed out");
    /// }
    /// # }
    /// ```
    #[cfg(feature = "time")]
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        Timeout::new(self, timeout)
    }
}

impl<T: ?Sized> FutureExt for T where T: Future {}
