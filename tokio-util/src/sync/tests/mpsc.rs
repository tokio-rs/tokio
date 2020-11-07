// TODO: loom
use crate::sync::Sender;
use futures::future::poll_fn;
use tokio::sync::mpsc::channel;

#[test]
fn basic() {
    futures::executor::block_on(async move {
        let (tx, mut rx) = channel::<String>(1);
        let tx = Sender::new(tx);
        tokio::pin!(tx);
        poll_fn(|cx| tx.as_mut().poll_ready(cx)).await.unwrap();
        tx.send("hello".to_string());
        assert_eq!(rx.try_recv(), Ok("hello".to_string()))
    });
}
