#![warn(rust_2018_idioms)]
#![feature(async_await)]

use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready, clock};
use tokio_timer::timer::Handle;
use tokio_timer::Delay;

use std::time::{Duration, Instant};

#[test]
fn immediate_delay() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create `Delay` that elapsed immediately.
        let mut delay = Delay::new(clock.now());

        // Ready!
        assert_ready!(task.poll(&mut delay));

        // Turn the timer, it runs for the elapsed time
        clock.turn_for(ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(clock.advanced(), ms(1000));
    });
}

#[test]
fn delayed_delay_level_0() {
    let mut task = MockTask::new();

    for &i in &[1, 10, 60] {
        clock::mock(|clock| {
            // Create a `Delay` that elapses in the future
            let mut delay = Delay::new(clock.now() + ms(i));

            // The delay has not elapsed.
            assert_pending!(task.poll(&mut delay));

            clock.turn();
            assert_eq!(clock.advanced(), ms(i));

            assert_ready!(task.poll(&mut delay));
        });
    }
}

#[test]
fn sub_ms_delayed_delay() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        for _ in 0..5 {
            let deadline = clock.now() + Duration::from_millis(1) + Duration::new(0, 1);

            let mut delay = Delay::new(deadline);

            assert_pending!(task.poll(&mut delay));

            clock.turn();
            assert_ready!(task.poll(&mut delay));

            assert!(clock.now() >= deadline);

            clock.advance(Duration::new(0, 1));
        }
    });
}

#[test]
fn delayed_delay_wrapping_level_0() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        clock.turn_for(ms(5));
        assert_eq!(clock.advanced(), ms(5));

        let mut delay = Delay::new(clock.now() + ms(60));

        assert_pending!(task.poll(&mut delay));

        clock.turn();
        assert_eq!(clock.advanced(), ms(64));
        assert_pending!(task.poll(&mut delay));

        clock.turn();
        assert_eq!(clock.advanced(), ms(65));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn timer_wrapping_with_higher_levels() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Set delay to hit level 1
        let mut s1 = Delay::new(clock.now() + ms(64));
        assert_pending!(task.poll(&mut s1));

        // Turn a bit
        clock.turn_for(ms(5));

        // Set timeout such that it will hit level 0, but wrap
        let mut s2 = Delay::new(clock.now() + ms(60));
        assert_pending!(task.poll(&mut s2));

        // This should result in s1 firing
        clock.turn();
        assert_eq!(clock.advanced(), ms(64));

        assert_ready!(task.poll(&mut s1));
        assert_pending!(task.poll(&mut s2));

        clock.turn();
        assert_eq!(clock.advanced(), ms(65));

        assert_ready!(task.poll(&mut s2));
    });
}

#[test]
fn delay_with_deadline_in_past() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create `Delay` that elapsed immediately.
        let mut delay = Delay::new(clock.now() - ms(100));

        // Even though the delay expires in the past, it is not ready yet
        // because the timer must observe it.
        assert_ready!(task.poll(&mut delay));

        // Turn the timer, it runs for the elapsed time
        clock.turn_for(ms(1000));

        // The time has not advanced. The `turn` completed immediately.
        assert_eq!(clock.advanced(), ms(1000));
    });
}

#[test]
fn delayed_delay_level_1() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(234));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        // Turn the timer, this will wake up to cascade the timer down.
        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(192));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        // Turn the timer again
        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(234));

        // The delay has elapsed.
        assert_ready!(task.poll(&mut delay));
    });

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(234));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        // Turn the timer with a smaller timeout than the cascade.
        clock.turn_for(ms(100));
        assert_eq!(clock.advanced(), ms(100));

        assert_pending!(task.poll(&mut delay));

        // Turn the timer, this will wake up to cascade the timer down.
        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(192));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        // Turn the timer again
        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(234));

        // The delay has elapsed.
        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn creating_delay_outside_of_context() {
    let now = Instant::now();

    // This creates a delay outside of the context of a mock timer. This tests
    // that it will still expire.
    let mut delay = Delay::new(now + ms(500));
    let mut task = MockTask::new();

    clock::mock_at(now, |clock| {
        // This registers the delay with the timer
        assert_pending!(task.poll(&mut delay));

        // Wait some time... the timer is cascading
        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(448));

        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(500));

        // The delay has elapsed
        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn concurrently_set_two_timers_second_one_shorter() {
    let mut t1 = MockTask::new();
    let mut t2 = MockTask::new();

    clock::mock(|clock| {
        let mut delay1 = Delay::new(clock.now() + ms(500));
        let mut delay2 = Delay::new(clock.now() + ms(200));

        // The delay has not elapsed
        assert_pending!(t1.poll(&mut delay1));
        assert_pending!(t2.poll(&mut delay2));

        // Delay until a cascade
        clock.turn();
        assert_eq!(clock.advanced(), ms(192));

        // Delay until the second timer.
        clock.turn();
        assert_eq!(clock.advanced(), ms(200));

        // The shorter delay fires
        assert_ready!(t2.poll(&mut delay2));
        assert_pending!(t1.poll(&mut delay1));

        clock.turn();
        assert_eq!(clock.advanced(), ms(448));

        assert_pending!(t1.poll(&mut delay1));

        // Turn again, this time the time will advance to the second delay
        clock.turn();
        assert_eq!(clock.advanced(), ms(500));

        assert_ready!(t1.poll(&mut delay1));
    })
}

#[test]
fn short_delay() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(1));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        // Turn the timer, but not enough time will go by.
        clock.turn();

        // The delay has elapsed.
        assert_ready!(task.poll(&mut delay));

        // The time has advanced to the point of the delay elapsing.
        assert_eq!(clock.advanced(), ms(1));
    })
}

#[test]
fn sorta_long_delay() {
    const MIN_5: u64 = 5 * 60 * 1000;

    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(MIN_5));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        let cascades = &[262_144, 262_144 + 9 * 4096, 262_144 + 9 * 4096 + 15 * 64];

        for &elapsed in cascades {
            clock.turn();
            assert_eq!(clock.advanced(), ms(elapsed));

            assert_pending!(task.poll(&mut delay));
        }

        clock.turn();
        assert_eq!(clock.advanced(), ms(MIN_5));

        // The delay has elapsed.
        assert_ready!(task.poll(&mut delay));
    })
}

#[test]
fn very_long_delay() {
    const MO_5: u64 = 5 * 30 * 24 * 60 * 60 * 1000;

    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(MO_5));

        // The delay has not elapsed.
        assert_pending!(task.poll(&mut delay));

        let cascades = &[
            12_884_901_888,
            12_952_010_752,
            12_959_875_072,
            12_959_997_952,
        ];

        for &elapsed in cascades {
            clock.turn();
            assert_eq!(clock.advanced(), ms(elapsed));

            assert_pending!(task.poll(&mut delay));
        }

        // Turn the timer, but not enough time will go by.
        clock.turn();

        // The time has advanced to the point of the delay elapsing.
        assert_eq!(clock.advanced(), ms(MO_5));

        // The delay has elapsed.
        assert_ready!(task.poll(&mut delay));
    })
}

#[test]
#[should_panic]
fn greater_than_max() {
    const YR_5: u64 = 5 * 365 * 24 * 60 * 60 * 1000;

    let mut task = MockTask::new();

    clock::mock(|clock| {
        // Create a `Delay` that elapses in the future
        let mut delay = Delay::new(clock.now() + ms(YR_5));

        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(0));

        // boom
        let _ = task.poll(&mut delay);
    })
}

#[test]
fn unpark_is_delayed() {
    let mut t1 = MockTask::new();
    let mut t2 = MockTask::new();
    let mut t3 = MockTask::new();

    clock::mock(|clock| {
        let mut delay1 = Delay::new(clock.now() + ms(100));
        let mut delay2 = Delay::new(clock.now() + ms(101));
        let mut delay3 = Delay::new(clock.now() + ms(200));

        assert_pending!(t1.poll(&mut delay1));
        assert_pending!(t2.poll(&mut delay2));
        assert_pending!(t3.poll(&mut delay3));

        clock.park_for(ms(500));

        assert_eq!(clock.advanced(), ms(500));

        assert_ready!(t1.poll(&mut delay1));
        assert_ready!(t2.poll(&mut delay2));
        assert_ready!(t3.poll(&mut delay3));
    })
}

#[test]
fn set_timeout_at_deadline_greater_than_max_timer() {
    const YR_1: u64 = 365 * 24 * 60 * 60 * 1000;
    const YR_5: u64 = 5 * YR_1;

    let mut task = MockTask::new();

    clock::mock(|clock| {
        for _ in 0..5 {
            clock.turn_for(ms(YR_1));
        }

        let mut delay = Delay::new(clock.now() + ms(1));
        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(YR_5) + ms(1));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn reset_future_delay_before_fire() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        let mut delay = Delay::new(clock.now() + ms(100));

        assert_pending!(task.poll(&mut delay));

        delay.reset(clock.now() + ms(200));

        clock.turn();
        assert_eq!(clock.advanced(), ms(192));

        assert_pending!(task.poll(&mut delay));

        clock.turn();
        assert_eq!(clock.advanced(), ms(200));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn reset_past_delay_before_turn() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        let mut delay = Delay::new(clock.now() + ms(100));

        assert_pending!(task.poll(&mut delay));

        delay.reset(clock.now() + ms(80));

        clock.turn();
        assert_eq!(clock.advanced(), ms(64));

        assert_pending!(task.poll(&mut delay));

        clock.turn();
        assert_eq!(clock.advanced(), ms(80));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn reset_past_delay_before_fire() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        let mut delay = Delay::new(clock.now() + ms(100));

        assert_pending!(task.poll(&mut delay));
        clock.turn_for(ms(10));

        assert_pending!(task.poll(&mut delay));
        delay.reset(clock.now() + ms(80));

        clock.turn();
        assert_eq!(clock.advanced(), ms(64));

        assert_pending!(task.poll(&mut delay));

        clock.turn();
        assert_eq!(clock.advanced(), ms(90));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn reset_future_delay_after_fire() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        let mut delay = Delay::new(clock.now() + ms(100));

        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(64));

        clock.turn();
        assert_eq!(clock.advanced(), ms(100));

        assert_ready!(task.poll(&mut delay));

        delay.reset(clock.now() + ms(10));
        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(1000));
        assert_eq!(clock.advanced(), ms(110));

        assert_ready!(task.poll(&mut delay));
    });
}

#[test]
fn delay_with_default_handle() {
    let handle = Handle::default();
    let now = Instant::now();
    let mut task = MockTask::new();

    let mut delay = handle.delay(now + ms(1));

    clock::mock_at(now, |clock| {
        assert_pending!(task.poll(&mut delay));

        clock.turn_for(ms(1));

        assert_ready!(task.poll(&mut delay));
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
