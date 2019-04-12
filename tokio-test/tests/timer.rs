#[macro_use]
extern crate tokio_test;
extern crate futures;
extern crate tokio_timer;

use futures::Future;
use std::time::{Duration, Instant};
use tokio_test::clock::Clock;
use tokio_timer::Delay;

#[test]
fn basic() {
    let mut mock = Clock::new();

    mock.enter(|handle| {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut delay = Delay::new(deadline);

        assert_not_ready!(delay.poll());

        handle.advance(Duration::from_secs(2));

        assert_ready!(delay.poll());
    });
}
