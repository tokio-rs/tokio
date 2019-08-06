#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio;
use tokio::prelude::*;
// use tokio::sync::mpsc;
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

        Delay::new(when).await;
        assert!(Instant::now() >= when);

        tx.send(()).unwrap();
    });

    rt.shutdown_on_idle();

    rx.recv().unwrap();
}

#[test]
fn timer_with_current_thread_runtime() {
    use tokio::runtime::current_thread::Runtime;

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        let when = Instant::now() + Duration::from_millis(100);

        Delay::new(when).await;
        assert!(Instant::now() >= when);

        tx.send(()).unwrap();
    });

    rt.run().unwrap();
    rx.recv().unwrap();
}

#[tokio::test]
async fn starving() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct Starve(Delay, u64);

    impl Future for Starve {
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
    let starve = Starve(Delay::new(when), 0);

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
