use sdt::pin::Pin;
use std::future::Future;
use std::marker;
use std::task::{Context, Poll};

/// Future for the [`pending()`] function.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct Pending<T> {
    _data: marker::PhantomData<T>,
}

/// Creates a future which never resolves, representing a computation that never
/// finishes.
///
/// The returned future will forever return [`Poll::Pending`].
///
/// # Examples
///
/// ```no_run
/// use tokio::future;
///
/// #[tokio::main]
/// async fn main {
///     future::pending().await;
///     unreachable!();
/// }
/// ```
pub async fn pending() -> ! {
    Pending {
        _data: marker::PhantomData,
    }
    .await
}

impl<T> Future for Pending<T> {
    type Output = !;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        Poll::Pending
    }
}

impl<T> Unpin for Pending<T> {}
