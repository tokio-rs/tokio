#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready_eq, clock};
use tokio_timer::*;

use std::time::Duration;

#[test]
#[should_panic]
fn interval_zero_duration() {
    clock::mock(|clock| {
        let _ = Interval::new(clock.now(), ms(0));
    });
}

#[test]
fn usage() {
    let mut task = MockTask::new();

    clock::mock(|clock| {
        let start = clock.now();
        let mut int = Interval::new(start, ms(300));

        macro_rules! poll {
            () => {
                task.enter(|cx| int.poll_next(cx))
            };
        }

        assert_ready_eq!(poll!(), Some(start));
        assert_pending!(poll!());

        clock.advance(ms(100));
        assert_pending!(poll!());

        clock.advance(ms(200));
        assert_ready_eq!(poll!(), Some(start + ms(300)));
        assert_pending!(poll!());

        clock.advance(ms(400));
        assert_ready_eq!(poll!(), Some(start + ms(600)));
        assert_pending!(poll!());

        clock.advance(ms(500));
        assert_ready_eq!(poll!(), Some(start + ms(900)));
        assert_ready_eq!(poll!(), Some(start + ms(1200)));
        assert_pending!(poll!());
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
