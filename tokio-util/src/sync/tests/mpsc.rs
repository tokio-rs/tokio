// TODO: loom
use std::task::Context;

use crate::sync::Sender;
use futures::future::poll_fn;
use tokio::sync::mpsc::channel;

#[test]
fn basic() {
    futures::executor::block_on(async {
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
    futures::executor::block_on(async {
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

#[test]
fn disarm_during_pending() {
    futures::executor::block_on(async {
        let (tx, mut rx) = channel::<u32>(1);
        let tx2 = tx.clone();
        let tx = Sender::new(tx);
        tokio::pin!(tx);
        tx2.send(1).await.unwrap();
        for _ in 0..100 {
            assert!(tx
                .as_mut()
                .poll_ready(&mut Context::from_waker(futures::task::noop_waker_ref()))
                .is_pending());
        }
        assert_eq!(rx.try_recv(), Ok(1));
        assert_eq!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        );
        // cancel pending future
        tx.as_mut().disarm();
        poll_fn(|cx| tx.as_mut().poll_ready(cx)).await.unwrap();
        tx.disarm();
        assert!(tx2.try_send(2).is_ok());
    });
}
