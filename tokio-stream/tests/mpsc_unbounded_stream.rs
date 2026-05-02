use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[tokio::test]
async fn size_hint_stream_open() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = UnboundedReceiverStream::new(rx);

    assert_eq!(stream.size_hint(), (2, None));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, None));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, None));
}

#[tokio::test]
async fn size_hint_stream_closed() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = UnboundedReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (2, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn size_hint_sender_dropped() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = UnboundedReceiverStream::new(rx);
    drop(tx);

    assert_eq!(stream.size_hint(), (2, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[test]
fn size_hint_stream_instantly_closed() {
    let (_tx, rx) = mpsc::unbounded_channel::<i32>();

    let mut stream = UnboundedReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (0, Some(0)));
}
