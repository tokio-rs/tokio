#![warn(rust_2018_idioms)]

use tokio::sync::mpsc;
use tokio::timer::throttle::Throttle;
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready_eq, clock};

use std::time::Duration;

#[test]
fn throttle() {
    clock::mock(|clock| {
        let (mut tx, rx) = mpsc::unbounded_channel();
        let mut stream = task::spawn(Throttle::new(rx, ms(1)));

        assert_pending!(stream.poll_next());

        for i in 0..3 {
            tx.try_send(i).unwrap();
        }

        for i in 0..3 {
            assert_ready_eq!(stream.poll_next(), Some(i));
            assert_pending!(stream.poll_next());

            clock.advance(ms(1));
        }

        assert_pending!(stream.poll_next());
    });
}

#[test]
fn throttle_dur_0() {
    clock::mock(|_| {
        let (mut tx, rx) = mpsc::unbounded_channel();
        let mut stream = task::spawn(Throttle::new(rx, ms(0)));

        assert_pending!(stream.poll_next());

        for i in 0..3 {
            tx.try_send(i).unwrap();
        }

        for i in 0..3 {
            assert_ready_eq!(stream.poll_next(), Some(i));
        }

        assert_pending!(stream.poll_next());
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
