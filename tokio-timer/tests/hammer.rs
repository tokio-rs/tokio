#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio_current_thread::CurrentThread;
use tokio_executor::park::{Park, Unpark, UnparkThread};
use tokio_timer::{Delay, Timer};

use rand;
use rand::Rng;
use std::cmp;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Barrier};
use std::task::{Context, Poll};
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
                let mut exec = CurrentThread::new();
                let mut rng = rand::thread_rng();

                barrier.wait();

                for _ in 0..PER_THREAD {
                    let deadline =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));
                    let delay = handle.delay(deadline);

                    exec.spawn(async move {
                        delay.await;

                        let now = Instant::now();
                        assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                    });
                }

                // Run the logic
                exec.run().unwrap();

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
                let mut exec = CurrentThread::new();
                let mut rng = rand::thread_rng();

                barrier.wait();

                for _ in 0..PER_THREAD {
                    let timeout1 = Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));
                    let timeout2 = Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline = Instant::now() + cmp::min(timeout1, timeout2);

                    let delay = handle.delay(Instant::now() + timeout1);
                    let join = handle.timeout(delay, timeout2);

                    exec.spawn(async move {
                        let _ = join.await;

                        let now = Instant::now();
                        assert!(now >= deadline, "deadline greater by {:?}", deadline - now);
                    });
                }

                // Run the logic
                exec.run().unwrap();

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
                let mut exec = CurrentThread::new();
                let mut rng = rand::thread_rng();

                barrier.wait();

                for _ in 0..PER_THREAD {
                    let deadline1 =
                        Instant::now() + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline2 =
                        deadline1 + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    let deadline3 =
                        deadline2 + Duration::from_millis(rng.gen_range(MIN_DELAY, MAX_DELAY));

                    struct Select {
                        a: Option<Delay>,
                        b: Option<Delay>,
                    }

                    impl Future for Select {
                        type Output = Delay;

                        fn poll(
                            mut self: Pin<&mut Self>,
                            cx: &mut Context<'_>,
                        ) -> Poll<Self::Output> {
                            let res = Pin::new(self.a.as_mut().unwrap()).poll(cx);

                            if res.is_ready() {
                                return Poll::Ready(self.a.take().unwrap());
                            }

                            let res = Pin::new(self.b.as_mut().unwrap()).poll(cx);

                            if res.is_ready() {
                                return Poll::Ready(self.b.take().unwrap());
                            }

                            Poll::Pending
                        }
                    }

                    let s = Select {
                        a: Some(handle.delay(deadline1)),
                        b: Some(handle.delay(deadline2)),
                    };

                    exec.spawn(async move {
                        let mut delay = s.await;

                        let now = Instant::now();
                        assert!(
                            now >= deadline1,
                            "deadline greater by {:?}",
                            deadline1 - now
                        );

                        delay.reset(deadline3);
                        delay.await;
                    });
                }

                // Run the logic
                exec.run().unwrap();

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
