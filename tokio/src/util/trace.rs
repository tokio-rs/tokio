use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::Span;

// A future, stream, sink, or executor that has been instrumented with a `tracing` span.
pin_project! {
    #[derive(Debug, Clone)]
    pub(crate) struct Instrumented<T> {
        #[pin]
        inner: T,
        span: Span,
    }
}

impl<T: Future> Future for Instrumented<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let _enter = this.span.enter();
        this.inner.poll(cx)
    }
}

impl<T> Instrumented<T> {
    pub(crate) fn new(inner: T, span: Span) -> Self {
        Self { inner, span }
    }
}
