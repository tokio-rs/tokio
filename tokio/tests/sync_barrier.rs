#![allow(clippy::unnecessary_operation)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

use tokio::sync::Barrier;

use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

struct IsSend<T: Send>(T);
#[test]
fn barrier_future_is_send() {
    let b = Barrier::new(0);
    IsSend(b.wait());
}

#[test]
fn zero_does_not_block() {
    let b = Barrier::new(0);

    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr.is_leader());
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr.is_leader());
    }
}

#[test]
fn single() {
    let b = Barrier::new(1);

    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr.is_leader());
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr.is_leader());
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr.is_leader());
    }
}

#[test]
fn tango() {
    let b = Barrier::new(2);

    let mut w1 = spawn(b.wait());
    assert_pending!(w1.poll());

    let mut w2 = spawn(b.wait());
    let wr2 = assert_ready!(w2.poll());
    let wr1 = assert_ready!(w1.poll());

    assert!(wr1.is_leader() || wr2.is_leader());
    assert!(!(wr1.is_leader() && wr2.is_leader()));
}

#[test]
#[cfg(panic = "unwind")]
fn panicking_waker_does_not_break_barrier() {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Wake, Waker};

    struct PanickingWaker;

    impl Wake for PanickingWaker {
        fn wake(self: Arc<Self>) {
            panic!("waker panicked");
        }
    }

    let b = Barrier::new(2);

    // A waiter parks with a waker that panics when woken.
    let waker = Waker::from(Arc::new(PanickingWaker));
    let mut cx = Context::from_waker(&waker);
    let mut w1 = pin!(b.wait());
    assert_pending!(w1.as_mut().poll(&mut cx));

    // The leader hits the waiter's panicking waker while releasing the
    // barrier.
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut w2 = spawn(b.wait());
        w2.poll()
    }))
    .is_err());

    // The barrier must still hold back the next cohort until it is full.
    let mut w3 = spawn(b.wait());
    assert_pending!(w3.poll());

    let mut w4 = spawn(b.wait());
    let wr4 = assert_ready!(w4.poll());
    let wr3 = assert_ready!(w3.poll());
    assert!(wr3.is_leader() != wr4.is_leader());
}

#[test]
fn lots() {
    let b = Barrier::new(100);

    for _ in 0..10 {
        let mut wait = Vec::new();
        for _ in 0..99 {
            let mut w = spawn(b.wait());
            assert_pending!(w.poll());
            wait.push(w);
        }
        for w in &mut wait {
            assert_pending!(w.poll());
        }

        // pass the barrier
        let mut w = spawn(b.wait());
        let mut found_leader = assert_ready!(w.poll()).is_leader();
        for mut w in wait {
            let wr = assert_ready!(w.poll());
            if wr.is_leader() {
                assert!(!found_leader);
                found_leader = true;
            }
        }
        assert!(found_leader);
    }
}
