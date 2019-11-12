#![warn(rust_2018_idioms)]

use tokio::sync::oneshot;
use tokio::time::{self, Instant, Timeout};
use tokio_test::*;

use futures::future::pending;
use std::time::Duration;

#[tokio::test]
async fn simultaneous_deadline_future_completion() {
    // Create a future that is immediately ready
    let mut fut = task::spawn(Timeout::new_at(async {}, Instant::now()));

    // Ready!
    assert_ready_ok!(fut.poll());
}

#[tokio::test]
async fn completed_future_past_deadline() {
    // Wrap it with a deadline
    let mut fut = task::spawn(Timeout::new_at(async {}, Instant::now() - ms(1000)));

    // Ready!
    assert_ready_ok!(fut.poll());
}

#[tokio::test]
async fn future_and_deadline_in_future() {
    time::pause();

    // Not yet complete
    let (tx, rx) = oneshot::channel();

    // Wrap it with a deadline
    let mut fut = task::spawn(Timeout::new_at(rx, Instant::now() + ms(100)));

    assert_pending!(fut.poll());

    // Turn the timer, it runs for the elapsed time
    time::advance(ms(90)).await;

    assert_pending!(fut.poll());

    // Complete the future
    tx.send(()).unwrap();
    assert!(fut.is_woken());

    assert_ready_ok!(fut.poll()).unwrap();
}

#[tokio::test]
async fn future_and_timeout_in_future() {
    time::pause();

    // Not yet complete
    let (tx, rx) = oneshot::channel();

    // Wrap it with a deadline
    let mut fut = task::spawn(Timeout::new(rx, ms(100)));

    // Ready!
    assert_pending!(fut.poll());

    // Turn the timer, it runs for the elapsed time
    time::advance(ms(90)).await;

    assert_pending!(fut.poll());

    // Complete the future
    tx.send(()).unwrap();

    assert_ready_ok!(fut.poll()).unwrap();
}

#[tokio::test]
async fn deadline_now_elapses() {
    use futures::future::pending;

    time::pause();

    // Wrap it with a deadline
    let mut fut = task::spawn(Timeout::new_at(pending::<()>(), Instant::now()));

    // Factor in jitter
    // TODO: don't require this
    time::advance(ms(1)).await;

    assert_ready_err!(fut.poll());
}

#[tokio::test]
async fn deadline_future_elapses() {
    time::pause();

    // Wrap it with a deadline
    let mut fut = task::spawn(Timeout::new_at(pending::<()>(), Instant::now() + ms(300)));

    assert_pending!(fut.poll());

    time::advance(ms(301)).await;

    assert!(fut.is_woken());
    assert_ready_err!(fut.poll());
}

#[tokio::test]
async fn stream_and_timeout_in_future() {
    use tokio::sync::mpsc;

    time::pause();

    // Not yet complete
    let (mut tx, rx) = mpsc::unbounded_channel();

    // Wrap it with a deadline
    let mut stream = task::spawn(Timeout::new(rx, ms(100)));

    // Not ready
    assert_pending!(stream.poll_next());

    // Turn the timer, it runs for the elapsed time
    time::advance(ms(90)).await;

    assert_pending!(stream.poll_next());

    // Complete the future
    tx.try_send(()).unwrap();

    let item = assert_ready!(stream.poll_next());
    assert!(item.is_some());
}

#[tokio::test]
async fn idle_stream_timesout_periodically() {
    use tokio::sync::mpsc;

    time::pause();

    // Not yet complete
    let (_tx, rx) = mpsc::unbounded_channel::<()>();

    // Wrap it with a deadline
    let mut stream = task::spawn(Timeout::new(rx, ms(100)));

    // Not ready
    assert_pending!(stream.poll_next());

    // Turn the timer, it runs for the elapsed time
    time::advance(ms(101)).await;

    let v = assert_ready!(stream.poll_next()).unwrap();
    assert_err!(v);

    // Stream's timeout should reset
    assert_pending!(stream.poll_next());

    // Turn the timer, it runs for the elapsed time
    time::advance(ms(101)).await;
    let v = assert_ready!(stream.poll_next()).unwrap();
    assert_err!(v);
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
