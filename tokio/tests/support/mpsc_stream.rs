#![allow(dead_code)]

use async_stream::stream;
use tokio::sync::mpsc::{self, Sender, UnboundedSender};
use tokio_stream::Stream;

pub fn unbounded_channel_stream<T: Unpin>() -> (UnboundedSender<T>, impl Stream<Item = T>) {
    let (tx, mut rx) = mpsc::unbounded_channel();

    let stream = stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    };

    (tx, stream)
}

pub fn channel_stream<T: Unpin>(size: usize) -> (Sender<T>, impl Stream<Item = T>) {
    let (tx, mut rx) = mpsc::channel(size);

    let stream = stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    };

    (tx, stream)
}
