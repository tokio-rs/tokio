#![warn(rust_2018_idioms)]

use tokio::sync::oneshot;
use tokio::timer::*;
use tokio_test::task;
use tokio_test::{
    assert_err, assert_pending, assert_ready, assert_ready_err, assert_ready_ok, clock,
};

use std::time::Duration;

#[test]
fn simultaneous_deadline_future_completion() {
    clock::mock(|clock| {
        // Create a future that is immediately ready
        let mut fut = task::spawn(Timeout::new_at(async {}, clock.now()));

        // Ready!
        assert_ready_ok!(fut.poll());
    });
}

#[test]
fn completed_future_past_deadline() {
    clock::mock(|clock| {
        // Wrap it with a deadline
        let mut fut = task::spawn(Timeout::new_at(async {}, clock.now() - ms(1000)));

        // Ready!
        assert_ready_ok!(fut.poll());
    });
}

#[test]
fn future_and_deadline_in_future() {
    clock::mock(|clock| {
        // Not yet complete
        let (tx, rx) = oneshot::channel();

        // Wrap it with a deadline
        let mut fut = task::spawn(Timeout::new_at(rx, clock.now() + ms(100)));

        assert_pending!(fut.poll());

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(fut.poll());

        // Complete the future
        tx.send(()).unwrap();

        assert_ready_ok!(fut.poll()).unwrap();
    });
}

#[test]
fn future_and_timeout_in_future() {
    clock::mock(|clock| {
        // Not yet complete
        let (tx, rx) = oneshot::channel();

        // Wrap it with a deadline
        let mut fut = task::spawn(Timeout::new(rx, ms(100)));

        // Ready!
        assert_pending!(fut.poll());

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(fut.poll());

        // Complete the future
        tx.send(()).unwrap();

        assert_ready_ok!(fut.poll()).unwrap();
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
    clock::mock(|clock| {
        // Wrap it with a deadline
        let mut fut = task::spawn(Timeout::new_at(Empty, clock.now()));

        assert_ready_err!(fut.poll());
    });
}

#[test]
fn deadline_future_elapses() {
    clock::mock(|clock| {
        // Wrap it with a deadline
        let mut fut = task::spawn(Timeout::new_at(Empty, clock.now() + ms(300)));

        assert_pending!(fut.poll());

        clock.advance(ms(300));

        assert_ready_err!(fut.poll());
    });
}

#[test]
fn stream_and_timeout_in_future() {
    use tokio::sync::mpsc;

    clock::mock(|clock| {
        // Not yet complete
        let (mut tx, rx) = mpsc::unbounded_channel();

        // Wrap it with a deadline
        let mut stream = task::spawn(Timeout::new(rx, ms(100)));

        // Not ready
        assert_pending!(stream.poll_next());

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(90));

        assert_pending!(stream.poll_next());

        // Complete the future
        tx.try_send(()).unwrap();

        let item = assert_ready!(stream.poll_next());
        assert!(item.is_some());
    });
}

#[test]
fn idle_stream_timesout_periodically() {
    use tokio::sync::mpsc;

    clock::mock(|clock| {
        // Not yet complete
        let (_tx, rx) = mpsc::unbounded_channel::<()>();

        // Wrap it with a deadline
        let mut stream = task::spawn(Timeout::new(rx, ms(100)));

        // Not ready
        assert_pending!(stream.poll_next());

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(100));

        let v = assert_ready!(stream.poll_next()).unwrap();
        assert_err!(v);

        // Stream's timeout should reset
        assert_pending!(stream.poll_next());

        // Turn the timer, it runs for the elapsed time
        clock.advance(ms(100));
        let v = assert_ready!(stream.poll_next()).unwrap();
        assert_err!(v)
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
