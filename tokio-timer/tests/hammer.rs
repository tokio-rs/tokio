extern crate futures;
extern crate rand;
extern crate tokio_executor;
extern crate tokio_timer;

use tokio_executor::park::{Park, Unpark, UnparkThread};
use tokio_timer::*;

use futures::{Future, Stream};
use futures::stream::FuturesUnordered;
use rand::Rng;

use std::cmp;
use std::sync::{Arc, Barrier};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::thread;
use std::time::{Duration, Instant};

struct Signal {
    rem: AtomicUsize,
    unpark: UnparkThread,
}

#[test]
fn hammer_complete() {
    const ITERS: usize = 5;
    const THREADS: usize = 4;
    const PER_THREAD: usize = 40;
    const MIN_DELAY: u64 = 1;
    const MAX_DELAY: u64 = 5_000;

    for i in 0..ITERS {
        let mut timer = Timer::default();
        let handle = timer.handle();
        let barrier = Arc::new(Barrier::new(THREADS));

        let done = Arc::new(Signal {
            rem: AtomicUsize::new(THREADS),
            unpark: timer.get_ref().unpark(),
        });

        for _ in 0..THREADS {
            let handle = handle.clone();
            let barrier = barrier.clone();
            let done = done.clone();

            thread::spawn(move || {
                let mut exec = FuturesUnordered::new();
                let mut rng = rand::thread_rng();

                barrier.wait();

                for _ in 0..PER_THREAD {
                    let deadline = Instant::now() + Duration::from_millis(
                        rng.gen_range(MIN_DELAY, MAX_DELAY));

                    exec.push({
                        handle.sleep(deadline)
                            .and_then(move |_| {
                                let now = Instant::now();
                                assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                                assert!(now < deadline + Duration::from_millis(15), "deadline past by {:?}", now - deadline);
                                Ok(())
                            })
                    });
                }

                // Run the logic
                exec.for_each(|_| Ok(()))
                    .wait()
                    .unwrap();

                if 1 == done.rem.fetch_sub(1, SeqCst) {
                    done.unpark.unpark();
                }
            });
        }

        while done.rem.load(SeqCst) > 0 {
            timer.turn(None).unwrap();
        }
    }
}

#[test]
fn hammer_cancel() {
    const ITERS: usize = 5;
    const THREADS: usize = 4;
    const PER_THREAD: usize = 40;
    const MIN_DELAY: u64 = 1;
    const MAX_DELAY: u64 = 5_000;

    for i in 0..ITERS {
        println!("~~ ITER ~~");
        let mut timer = Timer::default();
        let handle = timer.handle();
        let barrier = Arc::new(Barrier::new(THREADS));

        let done = Arc::new(Signal {
            rem: AtomicUsize::new(THREADS),
            unpark: timer.get_ref().unpark(),
        });

        for _ in 0..THREADS {
            let handle = handle.clone();
            let barrier = barrier.clone();
            let done = done.clone();

            thread::spawn(move || {
                let mut exec = FuturesUnordered::new();
                let mut rng = rand::thread_rng();

                barrier.wait();

                for _ in 0..PER_THREAD {
                    let deadline1 = Instant::now() + Duration::from_millis(
                        rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline2 = Instant::now() + Duration::from_millis(
                        rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline = cmp::min(deadline1, deadline2);

                    let sleep = handle.sleep(deadline1);
                    let join = handle.deadline(sleep, deadline2);

                    exec.push({
                        join
                            .and_then(move |_| {
                                let now = Instant::now();
                                assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                                // TODO: Remove the next line
                                assert!(now < deadline + Duration::from_millis(15), "deadline past by {:?}", now - deadline);
                                Ok(())
                            })
                    });
                }

                // Run the logic
                exec
                    .or_else(|e| {
                        assert!(e.is_elapsed());
                        Ok::<_, ()>(())
                    })
                    .for_each(|_| Ok(()))
                    .wait()
                    .unwrap();

                if 1 == done.rem.fetch_sub(1, SeqCst) {
                    done.unpark.unpark();
                }
            });
        }

        while done.rem.load(SeqCst) > 0 {
            timer.turn(None).unwrap();
        }
    }
}
