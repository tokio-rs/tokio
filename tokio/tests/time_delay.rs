#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::time::{self, Duration, Instant};
use tokio_test::{assert_pending, task};

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
async fn immediate_delay() {
    time::pause();

    let now = Instant::now();

    // Ready!
    time::delay_until(now).await;
    assert_elapsed!(now, 0);
}

#[tokio::test]
async fn delayed_delay_level_0() {
    time::pause();

    for &i in &[1, 10, 60] {
        let now = Instant::now();

        time::delay_until(now + ms(i)).await;

        assert_elapsed!(now, i);
    }
}

#[tokio::test]
async fn sub_ms_delayed_delay() {
    time::pause();

    for _ in 0..5 {
        let now = Instant::now();
        let deadline = now + ms(1) + Duration::new(0, 1);

        time::delay_until(deadline).await;

        assert_elapsed!(now, 1);
    }
}

#[tokio::test]
async fn delayed_delay_wrapping_level_0() {
    time::pause();

    time::delay_for(ms(5)).await;

    let now = Instant::now();
    time::delay_until(now + ms(60)).await;

    assert_elapsed!(now, 60);
}

#[tokio::test]
async fn reset_future_delay_before_fire() {
    time::pause();

    let now = Instant::now();

    let mut delay = task::spawn(time::delay_until(now + ms(100)));
    assert_pending!(delay.poll());

    let mut delay = delay.into_inner();

    delay.reset(Instant::now() + ms(200));
    delay.await;

    assert_elapsed!(now, 200);
}

#[tokio::test]
async fn reset_past_delay_before_turn() {
    time::pause();

    let now = Instant::now();

    let mut delay = task::spawn(time::delay_until(now + ms(100)));
    assert_pending!(delay.poll());

    let mut delay = delay.into_inner();

    delay.reset(now + ms(80));
    delay.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_past_delay_before_fire() {
    time::pause();

    let now = Instant::now();

    let mut delay = task::spawn(time::delay_until(now + ms(100)));
    assert_pending!(delay.poll());

    let mut delay = delay.into_inner();

    time::delay_for(ms(10)).await;

    delay.reset(now + ms(80));
    delay.await;

    assert_elapsed!(now, 80);
}

#[tokio::test]
async fn reset_future_delay_after_fire() {
    time::pause();

    let now = Instant::now();
    let mut delay = time::delay_until(now + ms(100));

    (&mut delay).await;
    assert_elapsed!(now, 100);

    delay.reset(now + ms(110));
    delay.await;
    assert_elapsed!(now, 110);
}

#[test]
#[should_panic]
fn creating_delay_outside_of_context() {
    let now = Instant::now();

    // This creates a delay outside of the context of a mock timer. This tests
    // that it will panic.
    let _fut = time::delay_until(now + ms(500));
}

#[should_panic]
#[tokio::test]
async fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    time::delay_until(Instant::now() + ms(YR_5)).await;
}

const NUM_LEVELS: usize = 6;
const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;

#[should_panic]
#[tokio::test]
async fn exactly_max() {
    // TODO: this should not panic but `time::ms()` is acting up
    time::delay_for(ms(MAX_DURATION)).await;
}

#[tokio::test]
async fn no_out_of_bounds_close_to_max() {
    time::pause();
    time::delay_for(ms(MAX_DURATION - 1)).await;
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
