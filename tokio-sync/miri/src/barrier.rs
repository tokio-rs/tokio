#![warn(rust_2018_idioms)]

use tokio_sync::Barrier;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

fn main() {
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
