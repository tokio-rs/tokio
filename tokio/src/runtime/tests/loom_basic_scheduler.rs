use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;
use crate::runtime;
use crate::sync::oneshot::{self, Receiver};
use crate::task;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::task::{Context, Poll};

#[test]
fn block_on_num_polls() {
    loom::model(|| {
        let rt = runtime::Builder::new_current_thread().build().unwrap();

        let (tx, rx) = oneshot::channel();
        let num_polls = Arc::new(AtomicUsize::new(0));

        rt.spawn(async move {
            for _ in 0..70 {
                task::yield_now().await;
            }
            tx.send(()).unwrap();
        });

        rt.block_on(async {
            BlockedFuture {
                rx,
                num_polls: num_polls.clone(),
            }
            .await;
        });

        let result = num_polls.load(Acquire);
        assert_eq!(2, result);
    });
}

struct BlockedFuture {
    rx: Receiver<()>,
    num_polls: Arc<AtomicUsize>,
}

impl Future for BlockedFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.num_polls.fetch_add(1, Release);

        match Pin::new(&mut self.rx).poll(cx) {
            Poll::Pending => Poll::Pending,
            _ => Poll::Ready(()),
        }
    }
}
