use crate::Stream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::FusedStream;
use pin_project_lite::pin_project;

pin_project! {
    /// Stream for the [`map_while`](super::StreamExt::map_while) method.
    #[must_use = "streams do nothing unless polled"]
    pub struct MapWhile<St, F> {
        #[pin]
        stream: St,
        f: F,
        done: bool,
    }
}

impl<St, F> fmt::Debug for MapWhile<St, F>
where
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapWhile")
            .field("stream", &self.stream)
            .field("done", &self.done)
            .finish()
    }
}

impl<St, F> MapWhile<St, F> {
    pub(super) fn new(stream: St, f: F) -> Self {
        MapWhile {
            stream,
            f,
            done: false,
        }
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

impl<St, F, T> Stream for MapWhile<St, F>
where
    St: Stream,
    F: FnMut(St::Item) -> Option<T>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let me = self.project();
        if *me.done {
            return Poll::Ready(None);
        }

        let f = me.f;
        let done = me.done;
        me.stream.poll_next(cx).map(|opt| {
            let mapped = opt.and_then(f);
            if mapped.is_none() {
                *done = true;
            }
            mapped
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.done {
            return (0, Some(0));
        }

        let (_, upper) = self.stream.size_hint();
        (0, upper)
    }
}

impl<St, F> FusedStream for MapWhile<St, F>
where
    Self: Stream,
{
    fn is_terminated(&self) -> bool {
        self.done
    }
}
