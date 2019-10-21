#![warn(rust_2018_idioms)]

use tokio_sync::oneshot;
use tokio_test::task::MockTask;
use tokio_test::{
    assert_err, assert_pending, assert_ready, assert_ready_err, assert_ready_ok, clock,
};
use tokio_timer::*;

use std::time::Duration;

#[test]
fn simultaneous_deadline_future_completion() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Create a future that is immediately ready
        let fut = Box::pin(Timeout::new_at(async {}, clock.now()));

        // Ready!
        assert_ready_ok!(t.poll(fut));
    });
}

#[test]
fn completed_future_past_deadline() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Wrap it with a deadline
        let fut = Timeout::new_at(async {}, clock.now() - ms(1000));
        let fut = Box::pin(fut);

        // Ready!
        assert_ready_ok!(t.poll(fut));
    });
}

#[test]
fn future_and_deadline_in_future() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Not yet complete
        let (tx, rx) = oneshot::channel();

        // Wrap it with a deadline
        let mut fut = Timeout::new_at(rx, clock.now() + ms(100));

        assert_pending!(t.poll(&mut fut));

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(t.poll(&mut fut));

        // Complete the future
        tx.send(()).unwrap();

        assert_ready_ok!(t.poll(&mut fut)).unwrap();
    });
}

#[test]
fn future_and_timeout_in_future() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Not yet complete
        let (tx, rx) = oneshot::channel();

        // Wrap it with a deadline
        let mut fut = Timeout::new(rx, ms(100));

        // Ready!
        assert_pending!(t.poll(&mut fut));

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(t.poll(&mut fut));

        // Complete the future
        tx.send(()).unwrap();

        assert_ready_ok!(t.poll(&mut fut)).unwrap();
    });
}

struct Empty;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

impl Future for Empty {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        Poll::Pending
    }
}

#[test]
fn deadline_now_elapses() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Wrap it with a deadline
        let mut fut = Timeout::new_at(Empty, clock.now());

        assert_ready_err!(t.poll(&mut fut));
    });
}

#[test]
fn deadline_future_elapses() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Wrap it with a deadline
        let mut fut = Timeout::new_at(Empty, clock.now() + ms(300));

        assert_pending!(t.poll(&mut fut));

        clock.advance(ms(300));

        assert_ready_err!(t.poll(&mut fut));
    });
}

#[cfg(feature = "async-traits")]
macro_rules! poll {
    ($task:ident, $stream:ident) => {{
        use futures_core::Stream;
        $task.enter(|cx| Pin::new(&mut $stream).poll_next(cx))
    }};
}

#[test]
#[cfg(feature = "async-traits")]
fn stream_and_timeout_in_future() {
    use tokio_sync::mpsc;

    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Not yet complete
        let (mut tx, rx) = mpsc::unbounded_channel();

        // Wrap it with a deadline
        let mut stream = Timeout::new(rx, ms(100));

        // Not ready
        assert_pending!(poll!(t, stream));

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(poll!(t, stream));

        // Complete the future
        tx.try_send(()).unwrap();

        let item = assert_ready!(poll!(t, stream));
        assert!(item.is_some());
    });
}

#[test]
#[cfg(feature = "async-traits")]
fn idle_stream_timesout_periodically() {
    use tokio_sync::mpsc;

    let mut t = MockTask::new();

    clock::mock(|clock| {
        // Not yet complete
        let (_tx, rx) = mpsc::unbounded_channel::<()>();

        // Wrap it with a deadline
        let mut stream = Timeout::new(rx, ms(100));

        // Not ready
        assert_pending!(poll!(t, stream));

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(100));

        let v = assert_ready!(poll!(t, stream)).unwrap();
        assert_err!(v);

        // Stream's timeout should reset
        assert_pending!(poll!(t, stream));

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(100));
        let v = assert_ready!(poll!(t, stream)).unwrap();
        assert_err!(v)
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
