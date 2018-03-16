extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

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
        assert!(!sleep.poll().unwrap().is_ready());

        // Turn the timer, note the duration of 1 sec.
        turn(timer, ms(1000));

        // The sleep is now elapsed.
        assert!(sleep.poll().unwrap().is_ready());

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
        assert!(!sleep.poll().unwrap().is_ready());

        // Turn the timer, but not enough timee will go by.
        turn(timer, ms(100));

        // The sleep has not elapsed.
        assert!(!sleep.poll().unwrap().is_ready());

        // Turn the timer, the specified time is greater than the remaining time
        // on the sleep.
        turn(timer, ms(200));

        // The sleep has elapsed.
        assert!(sleep.poll().unwrap().is_ready());

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
        assert!(!sleep.poll().unwrap().is_ready());

        // Wait some time
        turn(timer, ms(1000));

        // The sleep has elapsed
        assert!(sleep.poll().unwrap().is_ready());

        // The full second has not
        assert!(time.advanced() < ms(900))
    });
}
