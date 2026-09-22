use futures::{task::noop_waker_ref, Stream, StreamExt};
use futures_core::FusedStream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[tokio::test]
async fn size_hint_stream_open() {
    let (tx, rx) = mpsc::channel(4);

    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();

    let mut stream = ReceiverStream::new(rx);

    assert!(!stream.is_terminated());
    assert_eq!(stream.size_hint(), (2, None));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, None));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, None));
}

#[tokio::test]
async fn size_hint_stream_closed() {
    let (tx, rx) = mpsc::channel(4);

    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();

    let mut stream = ReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (2, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn size_hint_sender_dropped() {
    let (tx, rx) = mpsc::channel(4);

    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();

    let mut stream = ReceiverStream::new(rx);
    drop(tx);

    assert!(!stream.is_terminated());
    assert_eq!(stream.size_hint(), (2, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[test]
fn size_hint_stream_instantly_closed() {
    let (_tx, rx) = mpsc::channel::<i32>(4);

    let mut stream = ReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn size_hint_stream_closed_permits_send() {
    let (tx, rx) = mpsc::channel(4);

    tx.send(1).await.unwrap();
    let permit1 = tx.reserve().await.unwrap();
    let permit2 = tx.reserve().await.unwrap();

    let mut stream = ReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (1, Some(3)));
    permit1.send(2);
    assert_eq!(stream.size_hint(), (2, Some(3)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(1)));
    permit2.send(3);
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn size_hint_stream_closed_permits_drop() {
    let (tx, rx) = mpsc::channel(4);

    tx.send(1).await.unwrap();
    let permit1 = tx.reserve().await.unwrap();
    let permit2 = tx.reserve().await.unwrap();

    let mut stream = ReceiverStream::new(rx);
    stream.close();

    assert_eq!(stream.size_hint(), (1, Some(3)));
    drop(permit1);
    assert_eq!(stream.size_hint(), (1, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(1)));
    drop(permit2);
    assert_eq!(stream.size_hint(), (0, Some(0)));
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn fused_stream_termination_includes_outstanding_permit() {
    let (tx, rx) = mpsc::channel(1);
    let permit = tx.reserve().await.unwrap();
    let mut stream = ReceiverStream::new(rx);
    stream.close();

    assert!(!stream.is_terminated());
    let mut cx = Context::from_waker(noop_waker_ref());
    assert_eq!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Pending);
    permit.send(1);
    assert_eq!(stream.next().await, Some(1));
    assert!(!stream.is_terminated());
    assert_eq!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Ready(None));
    assert!(stream.is_terminated());
    assert_eq!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Ready(None));
}
