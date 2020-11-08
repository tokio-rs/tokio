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

#[test]
fn test_pending_and_disarm() {
    futures::executor::block_on(async move {
        let (tx, mut rx) = channel::<String>(1);
        let tx2 = tx.clone();
        let tx = Sender::new(tx);
        tokio::pin!(tx);
        poll_fn(|cx| tx.as_mut().poll_ready(cx)).await.unwrap();
        assert!(tx2.try_send("hi".to_string()).is_err());
        tx.as_mut().send("hello".to_string());
        assert_eq!(rx.try_recv(), Ok("hello".to_string()));
        assert_eq!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        );
        poll_fn(|cx| tx.as_mut().poll_ready(cx)).await.unwrap();
        tx.disarm();
        assert!(tx2.try_send("hi again".to_string()).is_ok());
    });
}
