//! An extension trait for Futures that provides a variety of convenient adapters.

mod with_cancellation_token;

use std::future::Future;

use with_cancellation_token::{WithCancellationTokenFuture, WithCancellationTokenFutureOwned};

use crate::sync::CancellationToken;

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
    fn timeout(self, timeout: std::time::Duration) -> tokio::time::Timeout<Self>
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
    fn timeout_at(self, deadline: tokio::time::Instant) -> tokio::time::Timeout<Self>
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
    ) -> WithCancellationTokenFuture<'_, Self>
    where
        Self: Sized,
    {
        WithCancellationTokenFuture::new(cancellation_token, self)
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
    ) -> WithCancellationTokenFutureOwned<Self>
    where
        Self: Sized,
    {
        WithCancellationTokenFutureOwned::new(cancellation_token, self)
    }
}

impl<T: Future + ?Sized> FutureExt for T {}
