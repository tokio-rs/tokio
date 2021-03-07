use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;
use crate::loom::thread;
use crate::runtime::{Builder, Runtime};
use crate::sync::oneshot::{self, Receiver};
use crate::task;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::task::{Context, Poll};

fn assert_num_polls(rt: Arc<Runtime>, at_most_polls: usize) {
    let (tx, rx) = oneshot::channel();
    let num_polls = Arc::new(AtomicUsize::new(0));

    rt.spawn(async move {
        for _ in 0..65 {
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

    let polls = num_polls.load(Acquire);
    assert!(polls <= at_most_polls);
}

#[test]
fn block_on_num_polls() {
    loom::model(|| {
        let rt1 = Arc::new(Builder::new_current_thread().build().unwrap());
        let rt2 = rt1.clone();

        let th1 = thread::spawn(|| assert_num_polls(rt1, 3));
        let th2 = thread::spawn(|| assert_num_polls(rt2, 3));

        th1.join().unwrap();
        th2.join().unwrap();
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
