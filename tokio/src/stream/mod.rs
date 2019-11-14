use std::pin::Pin;
use std::task::{Context, Poll};

/// Asynchronous iteration of values.
#[allow(unreachable_pub)]
pub trait Stream {
    type Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}
