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
use tokio::runtime::Runtime;

pin_project! {
    /// `TokioContext` allows connecting a custom executor with the tokio runtime.
    ///
    /// It contains a `Handle` to the runtime. A handle to the runtime can be
    /// obtain by calling the `Runtime::handle()` method.
    pub struct TokioContext<'a, F> {
        #[pin]
        inner: F,
        handle: &'a Runtime,
    }
}

impl<F: Future> Future for TokioContext<'_, F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        let handle = me.handle;
        let fut = me.inner;

        let _enter = handle.enter();
        fut.poll(cx)
    }
}

/// Trait extension that simplifies bundling a `Handle` with a `Future`.
pub trait RuntimeExt {
    /// Convenience method that takes a Future and returns a `TokioContext`.
    ///
    /// # Example: calling Tokio Runtime from a custom ThreadPool
    ///
    /// ```no_run
    /// use tokio_util::context::RuntimeExt;
    /// use tokio::time::{sleep, Duration};
    ///
    /// let rt = tokio::runtime::Builder::new_multi_thread()
    ///     .enable_all()
    ///     .build()
    ///     .unwrap();
    ///
    /// let rt2 = tokio::runtime::Builder::new_multi_thread()
    ///     .build()
    ///     .unwrap();
    ///
    /// let fut = sleep(Duration::from_millis(2));
    ///
    /// rt.block_on(
    ///     rt2
    ///         .wrap(async { sleep(Duration::from_millis(2)).await }),
    /// );
    ///```
    fn wrap<F: Future>(&self, fut: F) -> TokioContext<'_, F>;
}

impl RuntimeExt for Runtime {
    fn wrap<F: Future>(&self, fut: F) -> TokioContext<'_, F> {
        TokioContext {
            inner: fut,
            handle: self,
        }
    }
}
