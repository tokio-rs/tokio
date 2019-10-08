#![cfg(loom)]

#[path = ""]
mod thread_pool {
    const LOCAL_QUEUE_CAPACITY: usize = 2;

    #[path = "../src/thread_pool/queue/mod.rs"]
    #[allow(warnings)]
    pub(crate) mod queue;
}
use thread_pool::queue;

#[path = "../src/task/mod.rs"]
#[allow(warnings)]
mod task;
use task::{Header, Schedule, Task};

use loom;
use loom::thread;

use std::cell::Cell;
use std::rc::Rc;

#[allow(warnings)]
mod support {
    pub mod mock_schedule;
}
use support::mock_schedule::{Noop, NOOP_SCHEDULE};

#[test]
fn multi_worker() {
    const THREADS: usize = 2;
    const PER_THREAD: usize = 7;

    fn work(_i: usize, q: queue::Worker<Noop>, rem: Rc<Cell<usize>>) {
        let mut rem_local = PER_THREAD;

        while rem.get() != 0 {
            for _ in 0..3 {
                if rem_local > 0 {
                    q.push(val(0));
                    rem_local -= 1;
                }
            }

            // Try to work
            while let Some(task) = q.pop_local_first() {
                assert!(task.run(&NOOP_SCHEDULE).is_none());
                let r = rem.get();
                assert!(r > 0);
                rem.set(r - 1);
            }

            // Try to steal
            if let Some(task) = q.steal(0) {
                assert!(task.run(&NOOP_SCHEDULE).is_none());
                let r = rem.get();
                assert!(r > 0);
                rem.set(r - 1);
            }

            thread::yield_now();
        }
    }

    loom::model(|| {
        let rem = Rc::new(Cell::new(THREADS * PER_THREAD));

        let mut qs = queue::build(THREADS);
        let q1 = qs.remove(0);

        for i in 1..THREADS {
            let q = qs.remove(0);
            let rem = rem.clone();
            thread::spawn(move || {
                work(i, q, rem);
            });
        }

        work(0, q1, rem);

        // th.join().unwrap();
    });
}

fn val(num: u32) -> Task<Noop> {
    task::background(async move { num })
}
