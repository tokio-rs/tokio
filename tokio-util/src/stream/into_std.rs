use crate::stream::Stream;

use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`into_std`](super::Stream::into_std) method.
#[derive(Debug)]
pub struct IntoStd<T> {
    pub(super) stream: T,
}

impl<T: Stream> futures_core::Stream for IntoStd<T> {
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = unsafe { self.map_unchecked_mut(|me| &mut me.stream) };
        stream.poll_next(cx)
    }
}
