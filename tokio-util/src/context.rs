//! This module introduces `TokioContext`, which enables a simple way of using
//! the tokio runtime with any other executor.

use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::runtime::Handle;

pin_project! {
    /// TokioContext allows connecting a custom executor with the tokio runtime.
    ///
    /// It contains a `Handle` to the runtime. A handle to the runtime can be
    /// obtain by calling the `Runtime::handle()` method.
    pub struct TokioContext<F> {
        #[pin]
        inner: F,
        handle: Handle,
    }
}

impl<F: Future> Future for TokioContext<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        let handle = me.handle;
        let fut = me.inner;

        handle.enter(|| fut.poll(cx))
    }
}

/// Trait extension that simplifies bundling a `Handle` with a `Future`.
pub trait HandleExt: Into<Handle> + Clone {
    /// Convenience method that takes a Future and returns a `TokioContext`.
    ///
    /// # Example: calling Tokio Runtime from a custom ThreadPool
    ///
    ///
    /// ```no_run
    /// use tokio_util::context::HandleExt;
    /// use tokio::time::{delay_for, Duration};
    ///
    /// let mut rt = tokio::runtime::Builder::new()
    ///     .threaded_scheduler()
    ///     .enable_all()
    ///     .build().unwrap();
    ///
    /// let rt2 = tokio::runtime::Builder::new()
    ///     .threaded_scheduler()
    ///     .build().unwrap();
    ///
    /// let fut = delay_for(Duration::from_millis(2));
    ///
    /// rt.block_on(
    ///     rt2
    ///         .handle()
    ///         .wrap(async { delay_for(Duration::from_millis(2)).await }),
    /// );
    ///```
    fn wrap<F>(&self, fut: F) -> TokioContext<F>
    where
        F: Future,
    {
        TokioContext {
            inner: fut,
            handle: (*self).clone().into(),
        }
    }
}

impl HandleExt for Handle {}
