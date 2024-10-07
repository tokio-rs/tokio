use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Future for the [`unconstrained`](unconstrained) method.
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    #[must_use = "Unconstrained does nothing unless polled"]
    pub struct Unconstrained<F> {
        #[pin]
        inner: F,
    }
}

impl<F> Future for Unconstrained<F>
where
    F: Future,
{
    type Output = <F as Future>::Output;

    cfg_coop! {
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let inner = self.project().inner;
            crate::runtime::coop::with_unconstrained(|| inner.poll(cx))
        }
    }

    cfg_not_coop! {
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let inner = self.project().inner;
            inner.poll(cx)
        }
    }
}

impl<F: Future> Unconstrained<F>
where
    F: Future,
{
    /// Gets a reference to the wrapped future.
    pub fn get_ref(&self) -> &F {
        &self.inner
    }

    /// Gets a mutable reference to the wrapped future.
    pub fn get_mut(&mut self) -> &mut F {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the wrapped future.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut F> {
        self.project().inner
    }

    /// Consumes `Unconstrained`, returning the wrapped future.
    pub fn into_inner(self) -> F {
        self.inner
    }
}

/// Turn off cooperative scheduling for a future. The future will never be forced to yield by
/// Tokio. Using this exposes your service to starvation if the unconstrained future never yields
/// otherwise.
///
/// See also the usage example in the [task module](index.html#unconstrained).
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pub fn unconstrained<F>(inner: F) -> Unconstrained<F> {
    Unconstrained { inner }
}
