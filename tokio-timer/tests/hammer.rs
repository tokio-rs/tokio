extern crate futures;
extern crate rand;
extern crate tokio_executor;
extern crate tokio_timer;

use tokio_executor::park::{Park, Unpark, UnparkThread};
use tokio_timer::*;

use futures::stream::FuturesUnordered;
use futures::{Future, Stream};
use rand::Rng;

use std::cmp;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Barrier};
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

    for _ in 0..ITERS {
        let mut timer = Timer::default();
        let handle = timer.handle();
        let barrier = Arc::new(Barrier::new(THREADS));

        let done = Arc::new(Signal {
            rem: AtomicUsize::new(THREADS),
            unpark: timer.get_park().unpark(),
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
                    let deadline =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    exec.push({
                        handle.delay(deadline).and_then(move |_| {
                            let now = Instant::now();
                            assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                            Ok(())
                        })
                    });
                }

                // Run the logic
                exec.for_each(|_| Ok(())).wait().unwrap();

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

    for _ in 0..ITERS {
        let mut timer = Timer::default();
        let handle = timer.handle();
        let barrier = Arc::new(Barrier::new(THREADS));

        let done = Arc::new(Signal {
            rem: AtomicUsize::new(THREADS),
            unpark: timer.get_park().unpark(),
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
                    let deadline1 =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline2 =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline = cmp::min(deadline1, deadline2);

                    let delay = handle.delay(deadline1);
                    let join = handle.timeout(delay, deadline2);

                    exec.push({
                        join.and_then(move |_| {
                            let now = Instant::now();
                            assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                            Ok(())
                        })
                    });
                }

                // Run the logic
                exec.or_else(|e| {
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

#[test]
fn hammer_reset() {
    const ITERS: usize = 5;
    const THREADS: usize = 4;
    const PER_THREAD: usize = 40;
    const MIN_DELAY: u64 = 1;
    const MAX_DELAY: u64 = 250;

    for _ in 0..ITERS {
        let mut timer = Timer::default();
        let handle = timer.handle();
        let barrier = Arc::new(Barrier::new(THREADS));

        let done = Arc::new(Signal {
            rem: AtomicUsize::new(THREADS),
            unpark: timer.get_park().unpark(),
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
                    let deadline1 =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline2 =
                        deadline1 + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline3 =
                        deadline2 + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    exec.push({
                        handle
                            .delay(deadline1)
                            // Select over a second delay
                            .select2(handle.delay(deadline2))
                            .map_err(|e| panic!("boom; err={:?}", e))
                            .and_then(move |res| {
                                use futures::future::Either::*;

                                let now = Instant::now();
                                assert!(
                                    now >= deadline1,
                                    "deadline greater by {:?}",
                                    deadline1 - now
                                );

                                let mut other = match res {
                                    A((_, other)) => other,
                                    B((_, other)) => other,
                                };

                                other.reset(deadline3);
                                other
                            })
                            .and_then(move |_| {
                                let now = Instant::now();
                                assert!(
                                    now >= deadline3,
                                    "deadline greater by {:?}",
                                    deadline3 - now
                                );
                                Ok(())
                            })
                    });
                }

                // Run the logic
                exec.for_each(|_| Ok(())).wait().unwrap();

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
