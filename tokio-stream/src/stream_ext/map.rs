use crate::Stream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::FusedStream;
use pin_project_lite::pin_project;

pin_project! {
    /// Stream for the [`map`](super::StreamExt::map) method.
    #[must_use = "streams do nothing unless polled"]
    pub struct Map<St, F> {
        #[pin]
        stream: St,
        f: F,
    }
}

impl<St, F> fmt::Debug for Map<St, F>
where
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Map").field("stream", &self.stream).finish()
    }
}

impl<St, F> Map<St, F> {
    pub(super) fn new(stream: St, f: F) -> Self {
        Map { stream, f }
    }

    /// Returns a reference to the inner stream.
    pub fn get_ref(&self) -> &St {
        &self.stream
    }

    /// Returns a mutable reference to the inner stream.
    ///
    /// Mutating the inner stream may confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Returns a pinned mutable reference to the inner stream.
    ///
    /// Mutating the inner stream may confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().stream
    }

    /// Consumes this combinator and returns the inner stream.
    ///
    /// This may discard intermediate combinator state.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St, F, T> Stream for Map<St, F>
where
    St: Stream,
    F: FnMut(St::Item) -> T,
{
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.as_mut()
            .project()
            .stream
            .poll_next(cx)
            .map(|opt| opt.map(|x| (self.as_mut().project().f)(x)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St, F, T> FusedStream for Map<St, F>
where
    St: FusedStream,
    F: FnMut(St::Item) -> T,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}
