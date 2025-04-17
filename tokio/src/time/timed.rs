use std::{
    future::{Future, IntoFuture},
    pin::Pin,
    task::Poll,
};

use pin_project_lite::pin_project;

use crate::time::Instant;

/// Constructs a `Timed` future that wraps an underlying `Future`. The `Timed` future will return
/// the result of the underlying future as well as the duration the `Timed` was constructed for.
///
/// /// # Examples
///
/// Track how long we waited on a oneshot rx.
///
/// ```rust
/// use tokio::time::timed;
/// use tokio::sync::oneshot;
///
/// # async fn dox() {
/// let (tx, rx) = oneshot::channel();
/// # tx.send(()).unwrap();
///
/// // Wrap the future with a `Timed` to see how long we were at this step.
/// let (_received, duration) = timed(rx).await;
/// println!("Received a value after waiting {duration:?}");
/// # }
/// ```
pub fn timed<F>(future: F) -> Timed<F::IntoFuture>
where
    F: IntoFuture,
{
    Timed {
        inner: future.into_future(),
        start: Instant::now(),
    }
}

pin_project! {
    /// A helper future that wraps an inner future to also return how long the `Timed` was constructed.
    /// If constructed right before an await, it will give accurate timing information on the
    /// underlying future.
    ///
    /// # Examples
    ///
    /// Time a future.
    ///
    ///  ```
    ///  use tokio::time::timed;
    ///
    ///  async fn i_wish_i_knew_how_long_something_took_in_here() {
    ///    let (_, elapsed) = timed(async {
    ///       // do async work
    ///    }).await;
    ///  }
    ///  ```
    pub struct Timed<T> {
        #[pin]
        inner: T,
        start: Instant,
    }
}

impl<T> Future for Timed<T>
where
    T: Future,
{
    type Output = (T::Output, std::time::Duration);

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if let Poll::Ready(inner) = this.inner.poll(cx) {
            Poll::Ready((inner, this.start.elapsed()))
        } else {
            Poll::Pending
        }
    }
}
