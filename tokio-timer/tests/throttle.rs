extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

#[macro_use]
mod support;
use support::*;

use futures::{prelude::*, sync::mpsc};
use tokio_timer::throttle::Throttle;

#[test]
fn throttle() {
    mocked(|timer, _| {
        let (tx, rx) = mpsc::unbounded();
        let mut stream = Throttle::new(rx, ms(1));

        assert_not_ready!(stream);

        for i in 0..3 {
            tx.unbounded_send(i).unwrap();
        }
        for i in 0..3 {
            assert_ready_eq!(stream, Some(i));
            assert_not_ready!(stream);

            advance(timer, ms(1));
        }

        assert_not_ready!(stream);
    });
}

#[test]
fn throttle_dur_0() {
    mocked(|_, _| {
        let (tx, rx) = mpsc::unbounded();
        let mut stream = Throttle::new(rx, ms(0));

        assert_not_ready!(stream);

        for i in 0..3 {
            tx.unbounded_send(i).unwrap();
        }
        for i in 0..3 {
            assert_ready_eq!(stream, Some(i));
        }

        assert_not_ready!(stream);
    });
}
