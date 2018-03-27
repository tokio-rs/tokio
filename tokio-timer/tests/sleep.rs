extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

#[macro_use]
mod support;
use support::*;

use tokio_timer::*;

use futures::Future;

use std::time::{Duration, Instant};

#[test]
fn immediate_sleep() {
    mocked(|timer, time| {
        // Create `Sleep` that elapsed immediately.
        let mut sleep = Sleep::new(time.now());

        // Ready!
        assert_ready!(sleep);

        // Turn the timer, it runs for the elapsed time
        turn(timer, ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(time.advanced(), ms(1000));
    });
}

#[test]
fn delayed_sleep_level_0() {
    for &i in &[1, 10, 60] {
        mocked(|timer, time| {
            // Create a `Sleep` that elapses in the future
            let mut sleep = Sleep::new(time.now() + ms(i));

            // The sleep has not elapsed.
            assert_not_ready!(sleep);

            turn(timer, ms(1000));
            assert_eq!(time.advanced(), ms(i));

            assert_ready!(sleep);
        });
    }
}

#[test]
fn sub_ms_delayed_sleep() {
    mocked(|timer, time| {
        for _ in 0..5 {
            let deadline = time.now()
                + Duration::from_millis(1)
                + Duration::new(0, 1);

            let mut sleep = Sleep::new(deadline);

            assert_not_ready!(sleep);

            turn(timer, None);
            assert_ready!(sleep);

            assert!(time.now() >= deadline);

            time.advance(Duration::new(0, 1));
        }
    });
}

#[test]
fn delayed_sleep_wrapping_level_0() {
    mocked(|timer, time| {
        turn(timer, ms(5));
        assert_eq!(time.advanced(), ms(5));

        let mut sleep = Sleep::new(time.now() + ms(60));

        assert_not_ready!(sleep);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));
        assert_not_ready!(sleep);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(65));

        assert_ready!(sleep);
    });
}

#[test]
fn timer_wrapping_with_higher_levels() {
    mocked(|timer, time| {
        // Set sleep to hit level 1
        let mut s1 = Sleep::new(time.now() + ms(64));
        assert_not_ready!(s1);

        // Turn a bit
        turn(timer, ms(5));

        // Set timeout such that it will hit level 0, but wrap
        let mut s2 = Sleep::new(time.now() + ms(60));
        assert_not_ready!(s2);

        // This should result in s1 firing
        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));

        assert_ready!(s1);
        assert_not_ready!(s2);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(65));

        assert_ready!(s2);
    });
}

#[test]
fn sleep_with_deadline_in_past() {
    mocked(|timer, time| {
        // Create `Sleep` that elapsed immediately.
        let mut sleep = Sleep::new(time.now() - ms(100));

        // Even though the sleep expires in the past, it is not ready yet
        // because the timer must observe it.
        assert_ready!(sleep);

        // Turn the timer, it runs for the elapsed time
        turn(timer, ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(time.advanced(), ms(1000));
    });
}

#[test]
fn delayed_sleep_level_1() {
    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(234));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, this will wake up to cascade the timer down.
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(192));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer again
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(234));

        // The sleep has elapsed.
        assert_ready!(sleep);
    });

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(234));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer with a smaller timeout than the cascade.
        turn(timer, ms(100));
        assert_eq!(time.advanced(), ms(100));

        assert_not_ready!(sleep);

        // Turn the timer, this will wake up to cascade the timer down.
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(192));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer again
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(234));

        // The sleep has elapsed.
        assert_ready!(sleep);
    });
}

#[test]
fn creating_sleep_outside_of_context() {
    let now = Instant::now();

    // This creates a sleep outside of the context of a mock timer. This tests
    // that it will still expire.
    let mut sleep = Sleep::new(now + ms(500));

    mocked_with_now(now, |timer, time| {
        // This registers the sleep with the timer
        assert_not_ready!(sleep);

        // Wait some time... the timer is cascading
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(448));

        assert_not_ready!(sleep);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(500));

        // The sleep has elapsed
        assert_ready!(sleep);
    });
}

#[test]
fn concurrently_set_two_timers_second_one_shorter() {
    mocked(|timer, time| {
        let mut sleep1 = Sleep::new(time.now() + ms(500));
        let mut sleep2 = Sleep::new(time.now() + ms(200));

        // The sleep has not elapsed
        assert_not_ready!(sleep1);
        assert_not_ready!(sleep2);

        // Sleep until a cascade
        turn(timer, None);
        assert_eq!(time.advanced(), ms(192));

        // Sleep until the second timer.
        turn(timer, None);
        assert_eq!(time.advanced(), ms(200));

        // The shorter sleep fires
        assert_ready!(sleep2);
        assert_not_ready!(sleep1);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(448));

        assert_not_ready!(sleep1);

        // Turn again, this time the time will advance to the second sleep
        turn(timer, None);
        assert_eq!(time.advanced(), ms(500));

        assert_ready!(sleep1);
    })
}

#[test]
fn short_sleep() {
    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(1));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, but not enough timee will go by.
        turn(timer, None);

        // The sleep has elapsed.
        assert_ready!(sleep);

        // The time has advanced to the point of the sleep elapsing.
        assert_eq!(time.advanced(), ms(1));
    })
}

#[test]
fn sorta_long_sleep() {
    const MIN_5: u64 = 5 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(MIN_5));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        let cascades = &[
            262_144,
            262_144 + 9 * 4096,
            262_144 + 9 * 4096 + 15 * 64,
        ];

        for &elapsed in cascades {
            turn(timer, None);
            assert_eq!(time.advanced(), ms(elapsed));

            assert_not_ready!(sleep);
        }

        turn(timer, None);
        assert_eq!(time.advanced(), ms(MIN_5));

        // The sleep has elapsed.
        assert_ready!(sleep);
    })
}

#[test]
fn very_long_sleep() {
    const MO_5: u64 = 5 * 30 * 24 * 60 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(MO_5));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        let cascades = &[
            12_884_901_888,
            12_952_010_752,
            12_959_875_072,
            12_959_997_952,
        ];

        for &elapsed in cascades {
            turn(timer, None);
            assert_eq!(time.advanced(), ms(elapsed));

            assert_not_ready!(sleep);
        }

        // Turn the timer, but not enough time will go by.
        turn(timer, None);

        // The time has advanced to the point of the sleep elapsing.
        assert_eq!(time.advanced(), ms(MO_5));

        // The sleep has elapsed.
        assert_ready!(sleep);
    })
}

#[test]
fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(YR_5));

        assert_not_ready!(sleep);

        turn(timer, ms(0));

        assert!(sleep.poll().is_err());
    })
}

#[test]
fn unpark_is_delayed() {
    mocked(|timer, time| {
        let mut sleep1 = Sleep::new(time.now() + ms(100));
        let mut sleep2 = Sleep::new(time.now() + ms(101));
        let mut sleep3 = Sleep::new(time.now() + ms(200));

        assert_not_ready!(sleep1);
        assert_not_ready!(sleep2);
        assert_not_ready!(sleep3);

        time.park_for(ms(500));

        turn(timer, None);

        assert_eq!(time.advanced(), ms(500));

        assert_ready!(sleep1);
        assert_ready!(sleep2);
        assert_ready!(sleep3);
    })
}

#[test]
fn set_timeout_at_deadline_greater_than_max_timer() {
    const YR_1: u64 = 365 * 24 * 60 * 60 * 1000;
    const YR_5: u64 = 5 * YR_1;

    mocked(|timer, time| {
        for _ in 0..5 {
            turn(timer, ms(YR_1));
        }

        let mut sleep = Sleep::new(time.now() + ms(1));
        assert_not_ready!(sleep);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), Duration::from_millis(YR_5) + ms(1));

        assert_ready!(sleep);
    });
}

#[test]
fn reset_future_sleep_before_fire() {
    mocked(|timer, time| {
        let mut sleep = Sleep::new(time.now() + ms(100));

        assert_not_ready!(sleep);

        sleep.reset(time.now() + ms(200));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(192));

        assert_not_ready!(sleep);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(200));

        assert_ready!(sleep);
    });
}

#[test]
fn reset_past_sleep_before_fire() {
    mocked(|timer, time| {
        let mut sleep = Sleep::new(time.now() + ms(100));

        assert_not_ready!(sleep);

        sleep.reset(time.now() + ms(80));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));

        assert_not_ready!(sleep);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(80));

        assert_ready!(sleep);
    });
}

#[test]
fn reset_future_sleep_after_fire() {
    mocked(|timer, time| {
        let mut sleep = Sleep::new(time.now() + ms(100));

        assert_not_ready!(sleep);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(64));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(100));

        assert_ready!(sleep);

        sleep.reset(time.now() + ms(10));
        assert_not_ready!(sleep);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(110));

        assert_ready!(sleep);
    });
}
