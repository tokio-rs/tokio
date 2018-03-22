extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

#[macro_use]
mod support;

use futures::Future;
use tokio_timer::*;
use support::*;

use std::time::Instant;

#[test]
fn immediate_sleep() {
    mocked(|timer, time| {
        // Create `Sleep` that elapsed immediately.
        let mut sleep = Sleep::new(time.now());

        // Even though the sleep is effectively elapsed, the future is not yet
        // resolved. First, the timer must be turned.
        assert_not_ready!(sleep);

        // Turn the timer, note the duration of 1 sec.
        turn(timer, ms(1000));

        // The sleep is now elapsed.
        assert_ready!(sleep);

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(time.advanced(), ms(0));
    });
}

#[test]
fn delayed_sleep() {
    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(234));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, but not enough timee will go by.
        turn(timer, ms(100));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, the specified time is greater than the remaining time
        // on the sleep.
        turn(timer, ms(200));

        // The sleep has elapsed.
        assert_ready!(sleep);

        // The time has advanced to the point of the sleep elapsing.
        assert_eq!(time.advanced(), ms(234));
    })
}

#[test]
fn creating_sleep_outside_of_context() {
    // This creates a sleep outside of the context of a mock timer. This tests
    // that it will still expire.
    let mut sleep = Sleep::new(Instant::now() + ms(500));

    mocked(|timer, time| {
        // This registers the sleep with the timer
        assert_not_ready!(sleep);

        // Wait some time
        turn(timer, ms(1000));

        // The sleep has elapsed
        assert_ready!(sleep);

        // The full second has not
        assert!(time.advanced() < ms(900))
    });
}

#[test]
fn concurrently_set_two_timers_second_one_shorter() {
    mocked(|timer, time| {
        let mut sleep1 = Sleep::new(time.now() + ms(500));
        let mut sleep2 = Sleep::new(time.now() + ms(200));

        // The sleep has not elapsed
        assert!(!sleep1.poll().unwrap().is_ready());
        assert!(!sleep2.poll().unwrap().is_ready());

        // Sleeping goes until the second timer
        turn(timer, None);

        // Time advanced to the shortest timeout
        assert_eq!(time.advanced(), ms(200));

        // The shorter sleep fires
        assert_ready!(sleep2);
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
fn long_sleep() {
    const MIN_5: u64 = 5 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(MIN_5));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, but not enough time will go by.
        turn(timer, None);

        // The sleep has elapsed.
        assert_ready!(sleep);

        // The time has advanced to the point of the sleep elapsing.
        assert_eq!(time.advanced(), ms(MIN_5));
    })
}

#[test]
fn very_long_sleep() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    mocked(|timer, time| {
        // Create a `Sleep` that elapses in the future
        let mut sleep = Sleep::new(time.now() + ms(YR_5));

        // The sleep has not elapsed.
        assert_not_ready!(sleep);

        // Turn the timer, but not enough time will go by.
        turn(timer, None);

        // The sleep has elapsed.
        assert_ready!(sleep);

        // The time has advanced to the point of the sleep elapsing.
        assert_eq!(time.advanced(), ms(YR_5));
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
