#![warn(rust_2018_idioms)]

use tokio::time::*;
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready_eq, clock};

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
    clock::mock(|clock| {
        let start = clock.now();
        let mut int = task::spawn(Interval::new(start, ms(300)));

        assert_ready_eq!(int.poll_next(), Some(start));
        assert_pending!(int.poll_next());

        clock.advance(ms(100));
        assert_pending!(int.poll_next());

        clock.advance(ms(200));
        assert_ready_eq!(int.poll_next(), Some(start + ms(300)));
        assert_pending!(int.poll_next());

        clock.advance(ms(400));
        assert_ready_eq!(int.poll_next(), Some(start + ms(600)));
        assert_pending!(int.poll_next());

        clock.advance(ms(500));
        assert_ready_eq!(int.poll_next(), Some(start + ms(900)));
        assert_ready_eq!(int.poll_next(), Some(start + ms(1200)));
        assert_pending!(int.poll_next());
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
