#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

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

    let mut sleep = task::spawn(time::sleep_until(now + ms(100)));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    sleep.reset(Instant::now() + ms(200));
    sleep.await;

    assert_elapsed!(now, 200);
}

#[tokio::test]
async fn reset_past_sleep_before_turn() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(time::sleep_until(now + ms(100)));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    sleep.reset(now + ms(80));
    sleep.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_past_sleep_before_fire() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(time::sleep_until(now + ms(100)));
    assert_pending!(sleep.poll());

    let mut sleep = sleep.into_inner();

    time::sleep(ms(10)).await;

    sleep.reset(now + ms(80));
    sleep.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_future_sleep_after_fire() {
    time::pause();

    let now = Instant::now();
    let mut sleep = time::sleep_until(now + ms(100));

    (&mut sleep).await;
    assert_elapsed!(now, 100);

    sleep.reset(now + ms(110));
    sleep.await;
    assert_elapsed!(now, 110);
}

#[tokio::test]
async fn reset_sleep_to_past() {
    time::pause();

    let now = Instant::now();

    let mut sleep = task::spawn(time::sleep_until(now + ms(100)));
    assert_pending!(sleep.poll());

    time::sleep(ms(50)).await;

    assert!(!sleep.is_woken());

    sleep.reset(now + ms(40));

    assert!(sleep.is_woken());

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

#[should_panic]
#[tokio::test]
async fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    time::sleep_until(Instant::now() + ms(YR_5)).await;
}

const NUM_LEVELS: usize = 6;
const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;

#[should_panic]
#[tokio::test]
async fn exactly_max() {
    // TODO: this should not panic but `time::ms()` is acting up
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
