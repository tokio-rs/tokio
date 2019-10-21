#![warn(rust_2018_idioms)]

use tokio::timer::delay;
use tokio_test::clock::MockClock;
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready};

use std::time::{Duration, Instant};

#[test]
fn clock() {
    let mut mock = MockClock::new();

    mock.enter(|handle| {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut delay = task::spawn(delay(deadline));

        assert_pending!(delay.poll());

        handle.advance(Duration::from_secs(2));

        assert!(delay.is_woken());
        assert_ready!(delay.poll());
    });
}
