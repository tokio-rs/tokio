use tokio_stream::StreamExt;

#[tokio::test]
async fn map_while_yields_until_closure_returns_none() {
    let mut stream =
        tokio_stream::iter(1..=10).map_while(|x| if x < 4 { Some(x + 3) } else { None });
    assert_eq!(stream.next().await, Some(4));
    assert_eq!(stream.next().await, Some(5));
    assert_eq!(stream.next().await, Some(6));
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn map_while_does_not_poll_after_closure_returns_none() {
    // Once the closure returns `None`, the underlying stream must not be polled
    // again, so the trailing `2` is never yielded.
    let mut stream =
        tokio_stream::iter(vec![1, 5, 2]).map_while(|x| if x < 3 { Some(x) } else { None });
    assert_eq!(stream.next().await, Some(1));
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.next().await, None);
}
