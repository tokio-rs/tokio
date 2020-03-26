use crate::park::Unpark;
use crate::time::driver::Inner;
use crate::time::Instant;

use loom::thread;

use std::sync::atomic::Ordering;
use std::sync::Arc;

struct MockUnpark;

impl Unpark for MockUnpark {
    fn unpark(&self) {}
}

#[test]
fn balanced_incr_and_decr() {
    const OPS: usize = 5;

    fn incr(inner: Arc<Inner>) {
        for _ in 0..OPS {
            inner.increment().expect("increment should not have failed");
            thread::yield_now();
        }
    }

    fn decr(inner: Arc<Inner>) {
        let mut ops_performed = 0;
        while ops_performed < OPS {
            if inner.num(Ordering::Relaxed) > 0 {
                ops_performed += 1;
                inner.decrement();
            }
            thread::yield_now();
        }
    }

    loom::model(|| {
        let unpark = Box::new(MockUnpark);
        let instant = Instant::now();

        let inner = Arc::new(Inner::new(instant, unpark));

        let incr_inner = inner.clone();
        let decr_inner = inner.clone();

        let incr_hndle = thread::spawn(move || incr(incr_inner));
        let decr_hndle = thread::spawn(move || decr(decr_inner));

        incr_hndle.join().expect("should never fail");
        decr_hndle.join().expect("should never fail");

        assert_eq!(inner.num(Ordering::SeqCst), 0);
    })
}
