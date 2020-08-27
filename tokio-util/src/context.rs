//! Tokio context aware futures utilities.
//!
//! This module includes utilities around integrating tokio with other runtimes
//! by allowing the context to be attached to futures. This allows spawning
//! futures on other executors while still using tokio to drive them. This
//! can be useful if you need to use a tokio based library in an executor/runtime
//! that does not provide a tokio context.

use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::runtime::Handle;

pin_project! {
    /// `TokioContext` allows connecting a custom executor with the tokio runtime.
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
pub trait HandleExt {
    /// Convenience method that takes a Future and returns a `TokioContext`.
    ///
    /// # Example: calling Tokio Runtime from a custom ThreadPool
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
    fn wrap<F: Future>(&self, fut: F) -> TokioContext<F>;
}

impl HandleExt for Handle {
    fn wrap<F: Future>(&self, fut: F) -> TokioContext<F> {
        TokioContext {
            inner: fut,
            handle: self.clone(),
        }
    }
}
