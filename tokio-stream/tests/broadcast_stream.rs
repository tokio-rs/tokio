use futures_core::Stream;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

#[tokio::test]
async fn size_hint_with_alive_sender() {
    let (tx, rx) = broadcast::channel(16);
    tx.send(1).unwrap();
    tx.send(2).unwrap();

    let mut stream = BroadcastStream::new(rx);
    assert_eq!(stream.size_hint().0, 2);
    stream.next().await;
    assert_eq!(stream.size_hint().0, 1);
}

#[tokio::test]
async fn size_hint_no_sender_but_retained_values() {
    let (tx, rx) = broadcast::channel(16);
    tx.send(10).unwrap();
    tx.send(20).unwrap();
    drop(tx); // close all senders

    let mut stream = BroadcastStream::new(rx);
    assert_eq!(stream.size_hint().0, 2);
    stream.next().await;
    assert_eq!(stream.size_hint().0, 1);
    stream.next().await;
    assert_eq!(stream.size_hint().0, 0);
}

#[tokio::test]
async fn size_hint_no_sender_and_empty_channel() {
    let (_tx, rx) = broadcast::channel::<i32>(16);
    drop(_tx); // drop sender before sending anything

    let mut stream = BroadcastStream::new(rx);
    assert_eq!(stream.size_hint().0, 0);
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.size_hint().0, 0);
}
