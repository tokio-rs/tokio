//! An extension trait for Futures that provides a variety of convenient adapters.

use std::{future::Future, time::Duration};

use tokio::time::{Instant, Timeout};

use crate::sync::{CancellationToken, RunUntilCancelledFuture, RunUntilCancelledFutureOwned};

/// A trait which contains a variety of convenient adapters and utilities for `Future`s.
pub trait FutureExt: Future {
    /// A wrapper around [`tokio::time::timeout`], with the advantage that it is easier to write
    /// fluent call chains.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio::{sync::oneshot, time::Duration};
    /// use tokio_util::future::FutureExt;
    ///
    /// # async fn dox() {
    /// let (tx, rx) = oneshot::channel::<()>();
    ///
    /// let res = rx.timeout(Duration::from_millis(10)).await;
    /// assert!(res.is_err());
    /// # }
    /// ```
    #[cfg(feature = "time")]
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        tokio::time::timeout(timeout, self)
    }

    /// A wrapper around [`tokio::time::timeout_at`], with the advantage that it is easier to write
    /// fluent call chains.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio::{sync::oneshot, time::{Duration, Instant}};
    /// use tokio_util::future::FutureExt;
    ///
    /// # async fn dox() {
    /// let (tx, rx) = oneshot::channel::<()>();
    /// let deadline = Instant::now() + Duration::from_millis(10);
    ///
    /// let res = rx.timeout_at(deadline).await;
    /// assert!(res.is_err());
    /// # }
    /// ```
    #[cfg(feature = "time")]
    fn timeout_at(self, deadline: Instant) -> Timeout<Self>
    where
        Self: Sized,
    {
        tokio::time::timeout_at(deadline, self)
    }

    /// A wrapper around [`CancellationToken::run_until_cancelled`], with the advantage that it is easier to write
    /// fluent call chains.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio::sync::oneshot;
    /// use tokio_util::future::FutureExt;
    /// use tokio_util::sync::CancellationToken;
    ///
    /// async fn dox() {
    /// let (tx, rx) = oneshot::channel::<()>();
    /// let token = CancellationToken::new();
    /// let child_token = token.child_token();
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    ///     token.cancel();
    /// });
    /// assert!(rx.with_cancellation_token(&child_token).await.is_none())
    /// }
    /// ```
    fn with_cancellation_token(
        self,
        cancellation_token: &CancellationToken,
    ) -> RunUntilCancelledFuture<'_, Self>
    where
        Self: Sized,
    {
        RunUntilCancelledFuture {
            cancellation: cancellation_token.cancelled(),
            future: self,
        }
    }

    /// A wrapper around [`CancellationToken::run_until_cancelled_owned`], with the advantage that it is easier to write
    /// fluent call chains.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio::sync::oneshot;
    /// use tokio_util::future::FutureExt;
    /// use tokio_util::sync::CancellationToken;
    ///
    /// # async fn dox() {
    /// let (tx, rx) = oneshot::channel::<()>();
    /// let token = CancellationToken::new();
    /// let child_token = token.child_token();
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    ///     token.cancel();
    /// });
    /// assert!(rx.with_cancellation_token_owned(child_token).await.is_none())
    /// # }
    /// ```
    fn with_cancellation_token_owned(
        self,
        cancellation_token: CancellationToken,
    ) -> RunUntilCancelledFutureOwned<Self>
    where
        Self: Sized,
    {
        RunUntilCancelledFutureOwned {
            cancellation: cancellation_token.cancelled_owned(),
            future: self,
        }
    }
}

impl<T: Future + ?Sized> FutureExt for T {}
