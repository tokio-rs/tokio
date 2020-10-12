cfg_trace! {
    cfg_rt! {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use pin_project_lite::pin_project;

        use tracing::Span;

        pin_project! {
            /// A future that has been instrumented with a `tracing` span.
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

        #[inline]
        pub(crate) fn task<F>(task: F, kind: &'static str) -> Instrumented<F> {
            let span = tracing::trace_span!(
                target: "tokio::task",
                "task",
                %kind,
                future = %std::any::type_name::<F>(),
            );
            Instrumented::new(task, span)
        }
    }
}

cfg_not_trace! {
    cfg_rt! {
        #[inline]
        pub(crate) fn task<F>(task: F, _: &'static str) -> F {
            // nop
            task
        }
    }
}
