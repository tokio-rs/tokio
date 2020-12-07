#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::future::Future;
use std::task::Context;

use futures::task::noop_waker_ref;

use tokio::time::{self, Duration, Instant};
use tokio_test::{assert_pending, assert_ready, task};

macro_rules! assert_elapsed {
    ($now:expr, $ms:expr) => {{
        let elapsed = $now.elapsed();
        let lower = ms($ms);

        // Handles ms rounding
        assert!(
            elapsed >= lower && elapsed <= lower + ms(1),
            "actual = {:?}, expected = {:?}",
            elapsed,
            lower
        );
    }};
}

#[tokio::test]
async fn immediate_sleep() {
    time::pause();

    let now = Instant::now();

    // Ready!
    time::sleep_until(now).await;
    assert_elapsed!(now, 0);
}

#[tokio::test]
async fn is_elapsed() {
    time::pause();

    let sleep = time::sleep(Duration::from_millis(50));

    tokio::pin!(sleep);

    assert!(!sleep.is_elapsed());

    assert!(futures::poll!(sleep.as_mut()).is_pending());

    assert!(!sleep.is_elapsed());

    sleep.as_mut().await;

    assert!(sleep.is_elapsed());
}

#[tokio::test]
async fn delayed_sleep_level_0() {
    time::pause();

    for &i in &[1, 10, 60] {
        let now = Instant::now();

        time::sleep_until(now + ms(i)).await;

        assert_elapsed!(now, i);
    }
}

#[tokio::test]
async fn sub_ms_delayed_sleep() {
    time::pause();

    for _ in 0..5 {
        let now = Instant::now();
        let deadline = now + ms(1) + Duration::new(0, 1);

        time::sleep_until(deadline).await;

        assert_elapsed!(now, 1);
    }
}

#[tokio::test]
async fn delayed_sleep_wrapping_level_0() {
    time::pause();

    time::sleep(ms(5)).await;

    let now = Instant::now();
    time::sleep_until(now + ms(60)).await;

    assert_elapsed!(now, 60);
}

#[tokio::test]
async fn reset_future_sleep_before_fire() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(Box::pin(time::sleep_until(now + ms(100))));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    sleep.as_mut().reset(Instant::now() + ms(200));
    sleep.await;

    assert_elapsed!(now, 200);
}

#[tokio::test]
async fn reset_past_sleep_before_turn() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(Box::pin(time::sleep_until(now + ms(100))));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    sleep.as_mut().reset(now + ms(80));
    sleep.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_past_sleep_before_fire() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(Box::pin(time::sleep_until(now + ms(100))));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    time::sleep(ms(10)).await;

    sleep.as_mut().reset(now + ms(80));
    sleep.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_future_sleep_after_fire() {
    time::pause();

    let now = Instant::now();
    let mut sleep = Box::pin(time::sleep_until(now + ms(100)));

    sleep.as_mut().await;
    assert_elapsed!(now, 100);

    sleep.as_mut().reset(now + ms(110));
    sleep.await;
    assert_elapsed!(now, 110);
}

#[tokio::test]
async fn reset_sleep_to_past() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(Box::pin(time::sleep_until(now + ms(100))));
    assert_pending!(sleep.poll());

    time::sleep(ms(50)).await;

    assert!(!sleep.is_woken());

    sleep.as_mut().reset(now + ms(40));

    // TODO: is this required?
    //assert!(sleep.is_woken());

    assert_ready!(sleep.poll());
}

#[test]
#[should_panic]
fn creating_sleep_outside_of_context() {
    let now = Instant::now();

    // This creates a delay outside of the context of a mock timer. This tests
    // that it will panic.
    let _fut = time::sleep_until(now + ms(500));
}

#[tokio::test]
async fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    time::pause();
    time::sleep_until(Instant::now() + ms(YR_5)).await;
}

#[tokio::test]
async fn short_sleeps() {
    for i in 0..10000 {
        if (i % 10) == 0 {
            eprintln!("=== {}", i);
        }
        tokio::time::sleep(std::time::Duration::from_millis(0)).await;
    }
}

#[tokio::test]
async fn multi_long_sleeps() {
    tokio::time::pause();

    for _ in 0..5u32 {
        tokio::time::sleep(Duration::from_secs(
            // about a year
            365 * 24 * 3600,
        ))
        .await;
    }

    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(
            // about 10 years
            10 * 365 * 24 * 3600,
        );

    tokio::time::sleep_until(deadline).await;

    assert!(tokio::time::Instant::now() >= deadline);
}

#[tokio::test]
async fn long_sleeps() {
    tokio::time::pause();

    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(
            // about 10 years
            10 * 365 * 24 * 3600,
        );

    tokio::time::sleep_until(deadline).await;

    assert!(tokio::time::Instant::now() >= deadline);
    assert!(tokio::time::Instant::now() <= deadline + Duration::from_millis(1));
}

#[tokio::test]
#[should_panic(expected = "Duration too far into the future")]
async fn very_long_sleeps() {
    tokio::time::pause();

    // Some platforms (eg macos) can't represent times this far in the future
    if let Some(deadline) = tokio::time::Instant::now().checked_add(Duration::from_secs(1u64 << 62))
    {
        tokio::time::sleep_until(deadline).await;
    } else {
        // make it pass anyway (we can't skip/ignore the test based on the
        // result of checked_add)
        panic!("Duration too far into the future (test ignored)")
    }
}

#[tokio::test]
async fn reset_after_firing() {
    let timer = tokio::time::sleep(std::time::Duration::from_millis(1));
    tokio::pin!(timer);

    let deadline = timer.deadline();

    timer.as_mut().await;
    assert_ready!(timer
        .as_mut()
        .poll(&mut Context::from_waker(noop_waker_ref())));
    timer
        .as_mut()
        .reset(tokio::time::Instant::now() + std::time::Duration::from_secs(600));

    assert_ne!(deadline, timer.deadline());

    assert_pending!(timer
        .as_mut()
        .poll(&mut Context::from_waker(noop_waker_ref())));
    assert_pending!(timer
        .as_mut()
        .poll(&mut Context::from_waker(noop_waker_ref())));
}

const NUM_LEVELS: usize = 6;
const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;

#[tokio::test]
async fn exactly_max() {
    time::pause();
    time::sleep(ms(MAX_DURATION)).await;
}

#[tokio::test]
async fn no_out_of_bounds_close_to_max() {
    time::pause();
    time::sleep(ms(MAX_DURATION - 1)).await;
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[tokio::test(flavor = "multi_thread")]
async fn hang_on_shutdown() {
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<()>();
    tokio::spawn(async move {
        tokio::task::block_in_place(|| sync_rx.recv().ok());
    });

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        drop(sync_tx);
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
