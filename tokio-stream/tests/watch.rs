#![cfg(feature = "sync")]

use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;

#[tokio::test]
async fn watch_stream_message_not_twice() {
    let (tx, rx) = watch::channel("hello");

    let mut counter = 0;
    let mut stream = WatchStream::new(rx).map(move |payload| {
        println!("{}", payload);
        if payload == "goodbye" {
            counter += 1;
        }
        if counter >= 2 {
            panic!("too many goodbyes");
        }
    });

    let task = tokio::spawn(async move { while stream.next().await.is_some() {} });

    // Send goodbye just once
    tx.send("goodbye").unwrap();

    drop(tx);
    task.await.unwrap();
}

#[tokio::test]
async fn watch_stream_from_rx() {
    let (tx, rx) = watch::channel("hello");

    let mut stream = WatchStream::from(rx);

    assert_eq!(stream.next().await.unwrap(), "hello");

    tx.send("bye").unwrap();

    assert_eq!(stream.next().await.unwrap(), "bye");
}

#[tokio::test]
async fn watch_stream_new_on_change() {
    let (tx, rx) = watch::channel("hello");

    let mut stream = WatchStream::new_on_changed(rx);

    let task = tokio::spawn(async move {
        let value = stream.next().await.unwrap();
        if (value != "bye") {
            panic!("received unexpected value: {:?}", value);
        }
    });

    tx.send("bye").unwrap();

    task.await.unwrap();
}
