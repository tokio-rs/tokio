#![allow(dead_code)]

use crate::Stream;

use core::fmt;
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

pub(super) struct Partition<St, F>
where
    St: Stream,
{
    stream: St,
    f: F,
    true_buffer: VecDeque<St::Item>,
    false_buffer: VecDeque<St::Item>,
    true_waker: Option<Waker>,
    false_waker: Option<Waker>,
}

impl<St, F> Partition<St, F>
where
    St: Stream,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            true_buffer: VecDeque::new(),
            false_buffer: VecDeque::new(),
            true_waker: None,
            false_waker: None,
        }
    }

    pub(super) fn split(self) -> (TruePartition<St, F>, FalsePartition<St, F>) {
        let partition = Arc::new(Mutex::new(self));
        let true_partition = TruePartition::new(partition.clone());
        let false_partition = FalsePartition::new(partition.clone());
        (true_partition, false_partition)
    }
}

/// A stream that only yields elements that satisfy a predicate.
///
/// This stream is produced by the [`StreamExt::partition`] method.
pub struct TruePartition<St: Stream, F> {
    partition: Arc<Mutex<Partition<St, F>>>,
}

impl<St, F> fmt::Debug for TruePartition<St, F>
where
    St: Stream,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TruePartition").finish_non_exhaustive()
    }
}

impl<St, F> TruePartition<St, F>
where
    St: Stream,
{
    fn new(partition: Arc<Mutex<Partition<St, F>>>) -> Self {
        Self { partition }
    }
}

impl<St, F> Stream for TruePartition<St, F>
where
    St: Stream + Unpin,
    F: FnMut(&St::Item) -> bool,
{
    type Item = St::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut partition = self.partition.lock().unwrap();
        if let Some(item) = partition.true_buffer.pop_front() {
            return Poll::Ready(Some(item));
        }

        match Pin::new(&mut partition.stream).poll_next(cx) {
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(item)) if (partition.f)(&item) => Poll::Ready(Some(item)),
            Poll::Ready(Some(item)) => {
                partition.false_buffer.push_back(item);
                partition.false_waker = Some(cx.waker().clone());
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Pending => {
                partition.true_waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// A stream that only yields elements that do not satisfy a predicate.
///
/// This stream is produced by the [`StreamExt::partition`] method.
pub struct FalsePartition<St: Stream, F> {
    partition: Arc<Mutex<Partition<St, F>>>,
}

impl<St, F> fmt::Debug for FalsePartition<St, F>
where
    St: Stream,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FalsePartition").finish_non_exhaustive()
    }
}

impl<St: Stream, F> FalsePartition<St, F> {
    fn new(partition: Arc<Mutex<Partition<St, F>>>) -> Self {
        Self { partition }
    }
}

impl<St, F> Stream for FalsePartition<St, F>
where
    St: Stream + Unpin,
    F: FnMut(&St::Item) -> bool,
{
    type Item = St::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut partition = self.partition.lock().unwrap();
        if let Some(item) = partition.false_buffer.pop_front() {
            return Poll::Ready(Some(item));
        }

        match Pin::new(&mut partition.stream).poll_next(cx) {
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(item)) if !(partition.f)(&item) => Poll::Ready(Some(item)),
            Poll::Ready(Some(item)) => {
                partition.true_buffer.push_back(item);
                partition.true_waker = Some(cx.waker().clone());
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Pending => {
                partition.false_waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
