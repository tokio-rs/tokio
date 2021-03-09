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
        for _ in 0..12 {
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
        // we expect at most 3 number of polls because there are
        // three points at which we poll the future. At any of these
        // points it can be ready:
        //
        // - when we fail to steal the parker and we block on a
        //   notification that it is available.
        //
        // - when we steal the parker and we schedule the future
        //
        // - when the future is woken up and we have ran the max
        //   number of tasks for the current tick or there are no
        //   more tasks to run.
        //
        let at_most_num_polls = 3;

        let rt1 = Arc::new(Builder::new_current_thread().build().unwrap());
        let rt2 = rt1.clone();
        let rt3 = rt1.clone();

        let th1 = thread::spawn(move || assert_num_polls(rt1, at_most_num_polls));
        let th2 = thread::spawn(move || assert_num_polls(rt2, at_most_num_polls));
        let th3 = thread::spawn(move || assert_num_polls(rt3, at_most_num_polls));

        th1.join().unwrap();
        th2.join().unwrap();
        th3.join().unwrap();
    });
}

#[test]
fn completes_both() {
    loom::model(|| {
        let rt1 = Arc::new(Builder::new_current_thread().build().unwrap());
        let rt2 = rt1.clone();

        rt1.spawn(async {});
        rt1.spawn(async {});
        rt1.spawn(async {});

        let th1 = thread::spawn(move || {
            rt1.block_on(async {
                PollNTimes { n: 3, polled: 0 }.await;
            });
        });

        let th2 = thread::spawn(move || {
            rt2.block_on(async {
                PollNTimes { n: 3, polled: 0 }.await;
            });
        });

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

struct PollNTimes {
    polled: usize,
    n: usize,
}

impl Future for PollNTimes {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.polled += 1;
        if self.polled == self.n {
            return Poll::Ready(());
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
