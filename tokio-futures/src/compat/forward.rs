//! Converts an 0.1 `Future` into a `std::future::Future`.
//!
use futures::{Async, Future};

use std::future::Future as StdFuture;
use std::pin::Pin;
use std::task::{Context, Poll as StdPoll};

/// Converts an 0.1 `Future` into a `std::future::Future`.
#[derive(Debug)]
pub struct Compat<T>(T);

pub(crate) fn convert_poll<T, E>(poll: Result<Async<T>, E>) -> StdPoll<Result<T, E>> {
    use futures::Async::{NotReady, Ready};

    match poll {
        Ok(Ready(val)) => StdPoll::Ready(Ok(val)),
        Ok(NotReady) => StdPoll::Pending,
        Err(err) => StdPoll::Ready(Err(err)),
    }
}

pub(crate) fn convert_poll_stream<T, E>(
    poll: Result<Async<Option<T>>, E>,
) -> StdPoll<Option<Result<T, E>>> {
    use futures::Async::{NotReady, Ready};

    match poll {
        Ok(Ready(Some(val))) => StdPoll::Ready(Some(Ok(val))),
        Ok(Ready(None)) => StdPoll::Ready(None),
        Ok(NotReady) => StdPoll::Pending,
        Err(err) => StdPoll::Ready(Some(Err(err))),
    }
}

#[doc(hidden)]
pub trait IntoAwaitable {
    type Awaitable;

    /// Convert `self` into a value that can be used with `await!`.
    fn into_awaitable(self) -> Self::Awaitable;
}

impl<T: Future + Unpin> IntoAwaitable for T {
    type Awaitable = Compat<T>;

    fn into_awaitable(self) -> Self::Awaitable {
        Compat(self)
    }
}

impl<T> StdFuture for Compat<T>
where
    T: Future + Unpin,
{
    type Output = Result<T::Item, T::Error>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> StdPoll<Self::Output> {
        use futures::Async::{NotReady, Ready};

        // TODO: wire in cx

        match self.0.poll() {
            Ok(Ready(val)) => StdPoll::Ready(Ok(val)),
            Ok(NotReady) => StdPoll::Pending,
            Err(e) => StdPoll::Ready(Err(e)),
        }
    }
}
