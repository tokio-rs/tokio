#![warn(rust_2018_idioms)]
#![cfg(feature = "async-traits")]

use tokio_sync::mpsc;
use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready_eq, clock};
use tokio_timer::throttle::Throttle;

use futures_core::Stream;
use std::time::Duration;

macro_rules! poll {
    ($task:ident, $stream:ident) => {{
        use std::pin::Pin;
        $task.enter(|cx| Pin::new(&mut $stream).poll_next(cx))
    }};
}

#[test]
fn throttle() {
    let mut t = MockTask::new();

    clock::mock(|clock| {
        let (mut tx, rx) = mpsc::unbounded_channel();
        let mut stream = Throttle::new(rx, ms(1));

        assert_pending!(poll!(t, stream));

        for i in 0..3 {
            tx.try_send(i).unwrap();
        }

        for i in 0..3 {
            assert_ready_eq!(poll!(t, stream), Some(i));
            assert_pending!(poll!(t, stream));

            clock.advance(ms(1));
        }

        assert_pending!(poll!(t, stream));
    });
}

#[test]
fn throttle_dur_0() {
    let mut t = MockTask::new();

    clock::mock(|_| {
        let (mut tx, rx) = mpsc::unbounded_channel();
        let mut stream = Throttle::new(rx, ms(0));

        assert_pending!(poll!(t, stream));

        for i in 0..3 {
            tx.try_send(i).unwrap();
        }

        for i in 0..3 {
            assert_ready_eq!(poll!(t, stream), Some(i));
        }

        assert_pending!(poll!(t, stream));
    });
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
