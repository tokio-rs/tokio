
use futures::{Future, Async};
use futures_core::future::Future as Future03;
use futures_core::task::Poll as Poll03;

use std::marker::Unpin;
use std::mem::PinMut;
use std::task::Context;

/// Converts an 0.1 `Future` into an 0.3 `Future`.
#[derive(Debug)]
pub struct Compat<T>(T);

pub(crate) fn convert_poll<T, E>(poll: Result<Async<T>, E>) -> Poll03<Result<T, E>> {
    use futures::Async::{Ready, NotReady};

    match poll {
        Ok(Ready(val)) => Poll03::Ready(Ok(val)),
        Ok(NotReady) => Poll03::Pending,
        Err(err) => Poll03::Ready(Err(err)),
    }
}

pub(crate) fn convert_poll_stream<T, E>(
    poll: Result<Async<Option<T>>, E>) -> Poll03<Option<Result<T, E>>>
{
    use futures::Async::{Ready, NotReady};

    match poll {
        Ok(Ready(Some(val))) => Poll03::Ready(Some(Ok(val))),
        Ok(Ready(None)) => Poll03::Ready(None),
        Ok(NotReady) => Poll03::Pending,
        Err(err) => Poll03::Ready(Some(Err(err))),
    }
}

/// Convert a value into one that can be used with `await!`.
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

impl<T> Future03 for Compat<T>
where T: Future + Unpin
{
    type Output = Result<T::Item, T::Error>;

    fn poll(self: PinMut<Self>, _cx: &mut Context) -> Poll03<Self::Output> {
        use futures::Async::{Ready, NotReady};

        // TODO: wire in cx

        match PinMut::get_mut(self).0.poll() {
            Ok(Ready(val)) => Poll03::Ready(Ok(val)),
            Ok(NotReady) => Poll03::Pending,
            Err(e) => Poll03::Ready(Err(e)),
        }
    }
}
