use tokio_stream::{self as stream, Stream, StreamExt};
use tokio_test::{assert_pending, assert_ready, task};

use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn size_hint_without_peek() {
    let mut s = stream::iter(vec![1, 2, 3]).peekable();
    assert_eq!(s.size_hint(), (3, Some(3)));
    s.next().await;
    assert_eq!(s.size_hint(), (2, Some(2)));
    s.next().await;
    assert_eq!(s.size_hint(), (1, Some(1)));
    s.next().await;
    assert_eq!(s.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn size_hint_with_peek() {
    let mut s = stream::iter(vec![1, 2, 3]).peekable();

    // before peek: all items are in inner stream
    assert_eq!(s.size_hint(), (3, Some(3)));

    // after peek: one item moves into self.peek buffer — total must still be 3
    let _ = s.peek().await;
    assert_eq!(s.size_hint(), (3, Some(3)));

    // consume the peeked item via next()
    assert_eq!(s.next().await, Some(1));
    assert_eq!(s.size_hint(), (2, Some(2)));

    // peek again
    let _ = s.peek().await;
    assert_eq!(s.size_hint(), (2, Some(2)));

    s.next().await;
    assert_eq!(s.size_hint(), (1, Some(1)));

    // peek the last item
    let _ = s.peek().await;
    assert_eq!(s.size_hint(), (1, Some(1)));

    s.next().await;
    assert_eq!(s.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn size_hint_empty_stream() {
    let mut s = stream::iter(Vec::<i32>::new()).peekable();
    assert_eq!(s.size_hint(), (0, Some(0)));
    assert_eq!(s.peek().await, None);
    assert_eq!(s.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn peek_returns_correct_item() {
    let mut s = stream::iter(vec![10, 20, 30]).peekable();

    assert_eq!(s.peek().await, Some(&10));
    assert_eq!(s.peek().await, Some(&10)); // second peek returns same item
    assert_eq!(s.next().await, Some(10)); // next() gives same item
    assert_eq!(s.next().await, Some(20));
    assert_eq!(s.next().await, Some(30));
    assert_eq!(s.next().await, None);
}

#[tokio::test]
async fn size_hint_overflow() {
    // When inner stream reports usize::MAX upper bound and an item is peeked,
    // checked_add must return None rather than wrapping.
    struct MaxHint(bool); // bool = whether to return one item

    impl Stream for MaxHint {
        type Item = ();
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<()>> {
            if self.0 {
                self.0 = false;
                std::task::Poll::Ready(Some(()))
            } else {
                std::task::Poll::Ready(None)
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (usize::MAX, Some(usize::MAX))
        }
    }

    let mut s = MaxHint(true).peekable();

    // before peek: delegates directly to inner
    assert_eq!(s.size_hint(), (usize::MAX, Some(usize::MAX)));

    // after peek: peek_len=1, checked_add(1) on usize::MAX must give None not panic
    let _ = s.peek().await;
    assert_eq!(s.size_hint(), (usize::MAX, None));
}

#[tokio::test]
async fn size_hint_unbounded_upper() {
    // A stream that reports unknown upper bound
    struct Unbounded;

    impl Stream for Unbounded {
        type Item = u32;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<u32>> {
            std::task::Poll::Ready(Some(42))
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (1, None)
        }
    }

    let mut s = Unbounded.peekable();
    assert_eq!(s.size_hint(), (1, None));
    let _ = s.peek().await;
    assert_eq!(s.size_hint(), (2, None)); // still unbounded after peek
}

#[tokio::test]
async fn peek_does_not_consume() {
    let mut stream = stream::iter(vec![1, 2, 3]).peekable();

    assert_eq!(stream.peek().await, Some(&1));
    assert_eq!(stream.peek().await, Some(&1));
    assert_eq!(stream.next().await, Some(1));
    assert_eq!(stream.next().await, Some(2));
}

#[tokio::test]
async fn peek_mut_mutates_yielded_item() {
    let mut stream = stream::iter(vec![1, 2, 3]).peekable();

    if let Some(item) = stream.peek_mut().await {
        *item += 10;
    }

    assert_eq!(stream.next().await, Some(11));
    assert_eq!(stream.next().await, Some(2));
}

#[tokio::test]
async fn peek_on_empty_stream() {
    let mut stream = stream::iter(Vec::<i32>::new()).peekable();

    assert_eq!(stream.peek().await, None);
    assert_eq!(stream.peek_mut().await, None);
    assert_eq!(stream.next().await, None);
}

#[test]
fn poll_peek_does_not_advance_stream() {
    let mut stream = task::spawn(stream::iter(vec![1, 2, 3]).peekable());

    let first = stream.enter(|cx, s| s.poll_peek(cx).map(|opt| opt.copied()));
    assert_eq!(assert_ready!(first), Some(1));

    let second = stream.enter(|cx, s| s.poll_peek(cx).map(|opt| opt.copied()));
    assert_eq!(assert_ready!(second), Some(1));

    let next = stream.enter(|cx, s| s.poll_next(cx));
    assert_eq!(assert_ready!(next), Some(1));

    let next = stream.enter(|cx, s| s.poll_next(cx));
    assert_eq!(assert_ready!(next), Some(2));
}

#[test]
fn poll_peek_mutates_buffered_item() {
    let mut stream = task::spawn(stream::iter(vec![1, 2, 3]).peekable());

    stream.enter(|cx, s| {
        if let Poll::Ready(Some(item)) = s.poll_peek(cx) {
            *item += 100;
        }
    });

    let next = stream.enter(|cx, s| s.poll_next(cx));
    assert_eq!(assert_ready!(next), Some(101));
}

struct PendingOnce {
    polled: bool,
}

impl Stream for PendingOnce {
    type Item = i32;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<i32>> {
        if self.polled {
            Poll::Ready(Some(7))
        } else {
            self.polled = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[test]
fn poll_peek_propagates_pending() {
    let mut stream = task::spawn(PendingOnce { polled: false }.peekable());

    assert_pending!(stream.enter(|cx, s| s.poll_peek(cx).map(|opt| opt.copied())));

    let ready = stream.enter(|cx, s| s.poll_peek(cx).map(|opt| opt.copied()));
    assert_eq!(assert_ready!(ready), Some(7));
}
