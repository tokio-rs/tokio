// TODO: loom
use crate::sync::Sender;
use futures::future::poll_fn;
use tokio::sync::mpsc::channel;

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::pin!(fut);
    let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
    loop {
        if let std::task::Poll::Ready(r) = fut.as_mut().poll(&mut cx) {
            return r;
        }
    }
}

#[test]
fn basic() {
    block_on(async move {
        let (tx, mut rx) = channel::<String>(1);
        let tx = Sender::new(tx);
        tokio::pin!(tx);
        poll_fn(|cx| tx.as_mut().poll_ready(cx)).await.unwrap();
        tx.send("hello".to_string());
        assert_eq!(rx.try_recv(), Ok("hello".to_string()))
    });
}
