#![allow(dead_code)]

use crate::Stream;

use std::{
    fmt,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

/// A stream returned by the [`partition`](super::StreamExt::partition) method.
pub enum Partition<St, F>
where
    St: Stream,
{
    /// A stream that yields items for which the predicate returns `true`.
    Matches(Arc<Mutex<Inner<St, F>>>),
    /// A stream that yields items for which the predicate returns `false`.
    NonMatches(Arc<Mutex<Inner<St, F>>>),
}

impl<St, F> fmt::Debug for Partition<St, F>
where
    St: fmt::Debug + Stream,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Partition::Matches(inner) => f.debug_tuple("Partition::Matches").field(inner).finish(),
            Partition::NonMatches(inner) => {
                f.debug_tuple("Partition::NonMatches").field(inner).finish()
            }
        }
    }
}

impl<St, F> Partition<St, F>
where
    St: Stream + Unpin,
    F: FnMut(&St::Item) -> bool,
{
    pub(super) fn new(stream: St, f: F) -> (Self, Self) {
        let inner = Arc::new(Mutex::new(Inner::new(stream, f)));
        (
            Partition::Matches(inner.clone()),
            Partition::NonMatches(inner),
        )
    }
}

impl<St, F> Stream for Partition<St, F>
where
    St: Stream + Unpin,
    F: FnMut(&St::Item) -> bool,
{
    type Item = St::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Partition::Matches(inner) => inner.lock().unwrap().poll_next(cx, true),
            Partition::NonMatches(inner) => inner.lock().unwrap().poll_next(cx, false),
        }
    }
}

pub struct Inner<St, F>
where
    St: Stream,
{
    stream: St,
    f: F,
    buffered_value: Option<BufferedValue<St::Item>>,
    waker: Option<Waker>,
}

enum BufferedValue<T> {
    Match(T),
    NonMatch(T),
}

impl<St, F> fmt::Debug for Inner<St, F>
where
    St: fmt::Debug + Stream,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner")
            .field("stream", &self.stream)
            .field("waker", &self.waker)
            .finish_non_exhaustive()
    }
}

impl<St, F> Inner<St, F>
where
    St: Stream + Unpin,
    F: FnMut(&St::Item) -> bool,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            buffered_value: None,
            waker: None,
        }
    }

    fn poll_next(&mut self, cx: &mut Context<'_>, matches: bool) -> Poll<Option<St::Item>> {
        // Check if there is a buffered value
        match self.buffered_value.take() {
            Some(BufferedValue::Match(value)) if matches => return Poll::Ready(Some(value)),
            Some(BufferedValue::NonMatch(value)) if !matches => return Poll::Ready(Some(value)),
            Some(value) => {
                self.buffered_value = Some(value);
                self.waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
            None => {}
        }

        // Poll the underlying stream
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(value)) => match (self.f)(&value) {
                result if result == matches => Poll::Ready(Some(value)),
                true => {
                    self.buffered_value = Some(BufferedValue::Match(value));
                    self.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
                false => {
                    self.buffered_value = Some(BufferedValue::NonMatch(value));
                    self.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            },
            Poll::Ready(None) => Poll::Ready(None), // Stream is exhausted
            Poll::Pending => {
                self.waker = Some(cx.waker().clone());
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}
