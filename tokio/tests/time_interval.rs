#![warn(rust_2018_idioms)]

use tokio::time::{self, Duration, Instant, Interval};
use tokio_test::{assert_pending, assert_ready_eq, task};

#[tokio::test]
#[should_panic]
async fn interval_zero_duration() {
    let _ = Interval::new(Instant::now(), ms(0));
}

#[tokio::test]
async fn usage() {
    time::pause();

    let start = Instant::now();

    // TODO: Skip this
    time::advance(ms(1)).await;

    let mut int = task::spawn(Interval::new(start, ms(300)));

    assert_ready_eq!(int.poll_next(), Some(start));
    assert_pending!(int.poll_next());

    time::advance(ms(100)).await;
    assert_pending!(int.poll_next());

    time::advance(ms(200)).await;
    assert_ready_eq!(int.poll_next(), Some(start + ms(300)));
    assert_pending!(int.poll_next());

    time::advance(ms(400)).await;
    assert_ready_eq!(int.poll_next(), Some(start + ms(600)));
    assert_pending!(int.poll_next());

    time::advance(ms(500)).await;
    assert_ready_eq!(int.poll_next(), Some(start + ms(900)));
    assert_ready_eq!(int.poll_next(), Some(start + ms(1200)));
    assert_pending!(int.poll_next());
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
