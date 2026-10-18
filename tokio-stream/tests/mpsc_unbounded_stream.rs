use futures::{Stream, StreamExt};
use futures_core::FusedStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[tokio::test]
async fn size_hint_stream_open() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = UnboundedReceiverStream::new(rx);

    assert!(!stream.is_terminated());
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

    assert!(!stream.is_terminated());
    assert_eq!(stream.size_hint(), (2, None));
    assert_eq!(stream.next().await, Some(1));
    assert_eq!(stream.size_hint(), (1, None));
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.size_hint(), (0, None));
    assert!(!stream.is_terminated());
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.size_hint(), (0, Some(0)));
    assert!(stream.is_terminated());
}

#[tokio::test]
async fn size_hint_sender_dropped() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = UnboundedReceiverStream::new(rx);
    drop(tx);

    assert!(!stream.is_terminated());
    assert_eq!(stream.size_hint(), (2, None));
    assert_eq!(stream.next().await, Some(1));
    assert_eq!(stream.size_hint(), (1, None));
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.size_hint(), (0, None));
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.size_hint(), (0, Some(0)));
    assert!(stream.is_terminated());
}

#[tokio::test]
async fn size_hint_stream_instantly_closed() {
    let (_tx, rx) = mpsc::unbounded_channel::<i32>();

    let mut stream = UnboundedReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (0, None));
    assert!(!stream.is_terminated());
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.size_hint(), (0, Some(0)));
    assert!(stream.is_terminated());
}

#[tokio::test]
async fn fused_stream_terminates_when_sender_is_dropped() {
    let (tx, rx) = mpsc::unbounded_channel::<i32>();
    let mut stream = UnboundedReceiverStream::new(rx);
    assert!(!stream.is_terminated());
    drop(tx);

    assert!(!stream.is_terminated());
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn fused_stream_receiver_replaced_after_termination() {
    let (tx, rx) = mpsc::unbounded_channel::<i32>();
    let mut stream = UnboundedReceiverStream::new(rx);
    drop(tx);

    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
    assert_eq!(stream.size_hint(), (0, Some(0)));

    let (tx, rx) = mpsc::unbounded_channel();
    *stream.as_mut() = rx;

    assert!(!stream.is_terminated());
    assert_eq!(stream.size_hint(), (0, None));
    tx.send(1).unwrap();
    assert_eq!(stream.next().await, Some(1));

    stream.as_mut().close();
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
    assert_eq!(stream.size_hint(), (0, Some(0)));
}
