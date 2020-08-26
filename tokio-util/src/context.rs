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
    /// use std::futures::{self, Future};
    /// use tokio-utils::context::HandleExt;
    /// use tokio::runtime::{Builder, Handle, Runtime};
    /// use tokio::time::{delay_for, Duration};
    ///
    /// struct ThreadPool {
    ///     pub inner: futures::executor::ThreadPool,
    ///     pub rt: Runtime,
    /// }
    ///
    /// impl ThreadPool {
    ///     fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
    ///         let handle = self.rt.handle().clone();
    ///         // create a TokioContext from the handle and future.
    ///         let h = handle.wrap(f);
    ///         self.inner.spawn_ok(h);
    ///     }
    /// }
    ///
    /// let rt = tokio::runtime::Builder::new()
    ///     .threaded_scheduler()
    ///     .enable_all()
    ///     .build.unwrap();
    /// let inner = futures::executor::ThreadPool::builder().create().unwrap();
    /// let executor: ThreadPool {
    ///     inner, rt
    /// }
    ///
    /// executor.spawn(async {
    ///     delay_for(Duration::from_millis(2)).await
    /// });
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
