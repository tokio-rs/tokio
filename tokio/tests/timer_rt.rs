#![warn(rust_2018_idioms)]

use tokio::prelude::*;
use tokio::timer::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn timer_with_threaded_runtime() {
    use tokio::runtime::Runtime;

    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        let when = Instant::now() + Duration::from_millis(100);

        delay(when).await;
        assert!(Instant::now() >= when);

        tx.send(()).unwrap();
    });

    rx.recv().unwrap();
}

#[test]
fn timer_with_current_thread_runtime() {
    use tokio::runtime::Builder;

    let mut rt = Builder::new().current_thread().build().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.block_on(async move {
        let when = Instant::now() + Duration::from_millis(100);

        tokio::timer::delay(when).await;
        assert!(Instant::now() >= when);

        tx.send(()).unwrap();
    });

    rx.recv().unwrap();
}

#[tokio::test]
async fn starving() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct Starve<T: Future<Output = ()> + Unpin>(T, u64);

    impl<T: Future<Output = ()> + Unpin> Future for Starve<T> {
        type Output = u64;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u64> {
            if Pin::new(&mut self.0).poll(cx).is_ready() {
                return Poll::Ready(self.1);
            }

            self.1 += 1;

            cx.waker().wake_by_ref();

            Poll::Pending
        }
    }

    let when = Instant::now() + Duration::from_millis(20);
    let starve = Starve(delay(when), 0);

    starve.await;
    assert!(Instant::now() >= when);
}

#[tokio::test]
async fn timeout() {
    use tokio::sync::oneshot;

    let (_tx, rx) = oneshot::channel::<()>();

    let now = Instant::now();
    let dur = Duration::from_millis(20);

    let res = rx.timeout(dur).await;
    assert!(res.is_err());
    assert!(Instant::now() >= now + dur);
}
