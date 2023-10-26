#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::time::{Duration as StdDuration, Instant as StdInstant};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Duration, Instant};

#[tokio::test]
async fn pause_time_in_main() {
    time::set_time_auto_advance(false);
    time::pause();
}

#[tokio::test]
async fn pause_time_in_task() {
    let t = tokio::spawn(async {
        time::set_time_auto_advance(false);
        time::pause();
    });

    t.await.unwrap();
}

#[tokio::test(start_paused = true, time_auto_advance = false)]
async fn time_can_be_advanced_manually() {
    let std_start = StdInstant::now();
    let tokio_start = Instant::now();
    let (done_tx, mut done_rx) = oneshot::channel();

    let t = tokio::spawn(async {
        time::sleep(Duration::from_millis(100)).await;
        assert_eq!(done_tx.send(()), Ok(()));
    });

    // Simulated tokio time should not advance even real time does
    assert_eq!(tokio_start.elapsed(), Duration::ZERO);
    std::thread::sleep(StdDuration::from_millis(5));
    assert_eq!(tokio_start.elapsed(), Duration::ZERO);
    assert_eq!(done_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty));

    // The sleep shouldn't expire yet, but simulated time should advance
    time::advance(Duration::from_millis(50)).await;
    assert_eq!(tokio_start.elapsed(), Duration::from_millis(50));
    assert_eq!(done_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty));

    // Advance simulated time until the sleep expires
    time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await; // Make sure the scheduler picks up the task
    done_rx.try_recv().expect("Channel closed unexpectedly");

    t.await.unwrap();

    assert!(std_start.elapsed() < StdDuration::from_millis(100));
    assert_eq!(tokio_start.elapsed(), Duration::from_millis(150));
}

#[tokio::test(start_paused = true)]
async fn auto_advance_enable_disable_works() {
    let (stop_tx, mut stop_rx) = oneshot::channel();
    let (tx, mut rx) = mpsc::channel(1);

    let t = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_millis(100)).await;
            tx.send(()).await.expect("Send failed");

            let done = stop_rx.try_recv();
            if done == Err(oneshot::error::TryRecvError::Empty) {
                continue;
            }
            done.unwrap();
            break;
        }
    });

    // When the time is not paused, we should get new events constantly
    for _ in 0..10 {
        rx.recv().await.expect("Recv failed");
    }

    // Disable auto-advance and empty the buffer
    time::set_time_auto_advance(false);
    let _ = rx.try_recv();

    // Now we shouldn't be getting new events anymore
    for _ in 0..10 {
        tokio::task::yield_now().await;
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    }

    // Enable auto-advance and make sure we start receiving events again
    time::set_time_auto_advance(true);
    for _ in 0..10 {
        rx.recv().await.expect("Recv failed");
    }

    stop_tx.send(()).expect("Unable to send stop message");
    t.await.unwrap();
}
