use tokio_stream::{self as stream, Stream, StreamExt};

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
    assert_eq!(s.next().await, Some(10));  // next() gives same item
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
