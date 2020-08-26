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
    /// # Example
    ///
    /// use std::futures::Future;
    /// use tokio-utils::context::{HandleExt};
    /// use tokio::runtime::Handle;
    ///
    /// impl ThreadPool {
    /// fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
    ///     let handle = self.rt.handle().clone();
    ///     // create a TokioContext from the handle and future.
    ///     let h = handle.wrap(f);
    ///     self.inner.spawn_ok(h);
    /// }
    ///
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
