extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

#[macro_use]
mod support;
use support::*;

use tokio_timer::timer::Handle;
use tokio_timer::*;

use futures::Future;

use std::time::{Duration, Instant};

#[test]
fn immediate_delay() {
    mocked(|timer, time| {
        // Create `Delay` that elapsed immediately.
        let mut delay = Delay::new(time.now());

        // Ready!
        assert_ready!(delay);

        // Turn the timer, it runs for the elapsed time
        turn(timer, ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(time.advanced(), ms(1000));
    });
}

#[test]
fn delayed_delay_level_0() {
    for &i in &[1, 10, 60] {
        mocked(|timer, time| {
            // Create a `Delay` that elapses in the future
            let mut delay = Delay::new(time.now() + ms(i));

            // The delay has not elapsed.
            assert_not_ready!(delay);

            turn(timer, ms(1000));
            assert_eq!(time.advanced(), ms(i));

            assert_ready!(delay);
        });
    }
}

#[test]
fn sub_ms_delayed_delay() {
    mocked(|timer, time| {
        for _ in 0..5 {
            let deadline = time.now() + Duration::from_millis(1) + Duration::new(0, 1);

            let mut delay = Delay::new(deadline);

            assert_not_ready!(delay);

            turn(timer, None);
            assert_ready!(delay);

            assert!(time.now() >= deadline);

            time.advance(Duration::new(0, 1));
        }
    });
}

#[test]
fn delayed_delay_wrapping_level_0() {
    mocked(|timer, time| {
        turn(timer, ms(5));
        assert_eq!(time.advanced(), ms(5));

        let mut delay = Delay::new(time.now() + ms(60));

        assert_not_ready!(delay);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));
        assert_not_ready!(delay);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(65));

        assert_ready!(delay);
    });
}

#[test]
fn timer_wrapping_with_higher_levels() {
    mocked(|timer, time| {
        // Set delay to hit level 1
        let mut s1 = Delay::new(time.now() + ms(64));
        assert_not_ready!(s1);

        // Turn a bit
        turn(timer, ms(5));

        // Set timeout such that it will hit level 0, but wrap
        let mut s2 = Delay::new(time.now() + ms(60));
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
fn delay_with_deadline_in_past() {
    mocked(|timer, time| {
        // Create `Delay` that elapsed immediately.
        let mut delay = Delay::new(time.now() - ms(100));

        // Even though the delay expires in the past, it is not ready yet
        // because the timer must observe it.
        assert_ready!(delay);

        // Turn the timer, it runs for the elapsed time
        turn(timer, ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(time.advanced(), ms(1000));
    });
}

#[test]
fn delayed_delay_level_1() {
    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(234));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        // Turn the timer, this will wake up to cascade the timer down.
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(192));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        // Turn the timer again
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(234));

        // The delay has elapsed.
        assert_ready!(delay);
    });

    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(234));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        // Turn the timer with a smaller timeout than the cascade.
        turn(timer, ms(100));
        assert_eq!(time.advanced(), ms(100));

        assert_not_ready!(delay);

        // Turn the timer, this will wake up to cascade the timer down.
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(192));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        // Turn the timer again
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(234));

        // The delay has elapsed.
        assert_ready!(delay);
    });
}

#[test]
fn creating_delay_outside_of_context() {
    let now = Instant::now();

    // This creates a delay outside of the context of a mock timer. This tests
    // that it will still expire.
    let mut delay = Delay::new(now + ms(500));

    mocked_with_now(now, |timer, time| {
        // This registers the delay with the timer
        assert_not_ready!(delay);

        // Wait some time... the timer is cascading
        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(448));

        assert_not_ready!(delay);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(500));

        // The delay has elapsed
        assert_ready!(delay);
    });
}

#[test]
fn concurrently_set_two_timers_second_one_shorter() {
    mocked(|timer, time| {
        let mut delay1 = Delay::new(time.now() + ms(500));
        let mut delay2 = Delay::new(time.now() + ms(200));

        // The delay has not elapsed
        assert_not_ready!(delay1);
        assert_not_ready!(delay2);

        // Delay until a cascade
        turn(timer, None);
        assert_eq!(time.advanced(), ms(192));

        // Delay until the second timer.
        turn(timer, None);
        assert_eq!(time.advanced(), ms(200));

        // The shorter delay fires
        assert_ready!(delay2);
        assert_not_ready!(delay1);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(448));

        assert_not_ready!(delay1);

        // Turn again, this time the time will advance to the second delay
        turn(timer, None);
        assert_eq!(time.advanced(), ms(500));

        assert_ready!(delay1);
    })
}

#[test]
fn short_delay() {
    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(1));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        // Turn the timer, but not enough time will go by.
        turn(timer, None);

        // The delay has elapsed.
        assert_ready!(delay);

        // The time has advanced to the point of the delay elapsing.
        assert_eq!(time.advanced(), ms(1));
    })
}

#[test]
fn sorta_long_delay() {
    const MIN_5: u64 = 5 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(MIN_5));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        let cascades = &[262_144, 262_144 + 9 * 4096, 262_144 + 9 * 4096 + 15 * 64];

        for &elapsed in cascades {
            turn(timer, None);
            assert_eq!(time.advanced(), ms(elapsed));

            assert_not_ready!(delay);
        }

        turn(timer, None);
        assert_eq!(time.advanced(), ms(MIN_5));

        // The delay has elapsed.
        assert_ready!(delay);
    })
}

#[test]
fn very_long_delay() {
    const MO_5: u64 = 5 * 30 * 24 * 60 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(MO_5));

        // The delay has not elapsed.
        assert_not_ready!(delay);

        let cascades = &[
            12_884_901_888,
            12_952_010_752,
            12_959_875_072,
            12_959_997_952,
        ];

        for &elapsed in cascades {
            turn(timer, None);
            assert_eq!(time.advanced(), ms(elapsed));

            assert_not_ready!(delay);
        }

        // Turn the timer, but not enough time will go by.
        turn(timer, None);

        // The time has advanced to the point of the delay elapsing.
        assert_eq!(time.advanced(), ms(MO_5));

        // The delay has elapsed.
        assert_ready!(delay);
    })
}

#[test]
fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(time.now() + ms(YR_5));

        assert_not_ready!(delay);

        turn(timer, ms(0));

        assert!(delay.poll().is_err());
    })
}

#[test]
fn unpark_is_delayed() {
    mocked(|timer, time| {
        let mut delay1 = Delay::new(time.now() + ms(100));
        let mut delay2 = Delay::new(time.now() + ms(101));
        let mut delay3 = Delay::new(time.now() + ms(200));

        assert_not_ready!(delay1);
        assert_not_ready!(delay2);
        assert_not_ready!(delay3);

        time.park_for(ms(500));

        turn(timer, None);

        assert_eq!(time.advanced(), ms(500));

        assert_ready!(delay1);
        assert_ready!(delay2);
        assert_ready!(delay3);
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

        let mut delay = Delay::new(time.now() + ms(1));
        assert_not_ready!(delay);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), Duration::from_millis(YR_5) + ms(1));

        assert_ready!(delay);
    });
}

#[test]
fn reset_future_delay_before_fire() {
    mocked(|timer, time| {
        let mut delay = Delay::new(time.now() + ms(100));

        assert_not_ready!(delay);

        delay.reset(time.now() + ms(200));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(192));

        assert_not_ready!(delay);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(200));

        assert_ready!(delay);
    });
}

#[test]
fn reset_past_delay_before_turn() {
    mocked(|timer, time| {
        let mut delay = Delay::new(time.now() + ms(100));

        assert_not_ready!(delay);

        delay.reset(time.now() + ms(80));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));

        assert_not_ready!(delay);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(80));

        assert_ready!(delay);
    });
}

#[test]
fn reset_past_delay_before_fire() {
    mocked(|timer, time| {
        let mut delay = Delay::new(time.now() + ms(100));

        assert_not_ready!(delay);
        turn(timer, ms(10));

        assert_not_ready!(delay);
        delay.reset(time.now() + ms(80));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(64));

        assert_not_ready!(delay);

        turn(timer, None);
        assert_eq!(time.advanced(), ms(90));

        assert_ready!(delay);
    });
}

#[test]
fn reset_future_delay_after_fire() {
    mocked(|timer, time| {
        let mut delay = Delay::new(time.now() + ms(100));

        assert_not_ready!(delay);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(64));

        turn(timer, None);
        assert_eq!(time.advanced(), ms(100));

        assert_ready!(delay);

        delay.reset(time.now() + ms(10));
        assert_not_ready!(delay);

        turn(timer, ms(1000));
        assert_eq!(time.advanced(), ms(110));

        assert_ready!(delay);
    });
}

#[test]
fn delay_with_default_handle() {
    let handle = Handle::default();
    let now = Instant::now();

    let mut delay = handle.delay(now + ms(1));

    mocked_with_now(now, |timer, _time| {
        assert_not_ready!(delay);

        turn(timer, ms(1));

        assert_ready!(delay);
    });
}
