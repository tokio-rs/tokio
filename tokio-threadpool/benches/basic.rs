#![feature(test)]
#![deny(warnings)]

extern crate futures;
extern crate futures_cpupool;
extern crate num_cpus;
extern crate test;
extern crate tokio_threadpool;

const NUM_SPAWN: usize = 10_000;
const NUM_YIELD: usize = 1_000;
const TASKS_PER_CPU: usize = 50;

mod threadpool {
    use futures::{future, task, Async};
    use num_cpus;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;
    use std::sync::{mpsc, Arc};
    use test;
    use tokio_threadpool::*;

    #[bench]
    fn spawn_many(b: &mut test::Bencher) {
        let threadpool = ThreadPool::new();

        let (tx, rx) = mpsc::sync_channel(10);
        let rem = Arc::new(AtomicUsize::new(0));

        b.iter(move || {
            rem.store(super::NUM_SPAWN, SeqCst);

            for _ in 0..super::NUM_SPAWN {
                let tx = tx.clone();
                let rem = rem.clone();

                threadpool.spawn(future::lazy(move || {
                    if 1 == rem.fetch_sub(1, SeqCst) {
                        tx.send(()).unwrap();
                    }

                    Ok(())
                }));
            }

            let _ = rx.recv().unwrap();
        });
    }

    #[bench]
    fn yield_many(b: &mut test::Bencher) {
        let threadpool = ThreadPool::new();
        let tasks = super::TASKS_PER_CPU * num_cpus::get();

        let (tx, rx) = mpsc::sync_channel(tasks);

        b.iter(move || {
            for _ in 0..tasks {
                let mut rem = super::NUM_YIELD;
                let tx = tx.clone();

                threadpool.spawn(future::poll_fn(move || {
                    rem -= 1;

                    if rem == 0 {
                        tx.send(()).unwrap();
                        Ok(Async::Ready(()))
                    } else {
                        // Notify the current task
                        task::current().notify();

                        // Not ready
                        Ok(Async::NotReady)
                    }
                }));
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}

// In this case, CPU pool completes the benchmark faster, but this is due to how
// CpuPool currently behaves, starving other futures. This completes the
// benchmark quickly but results in poor runtime characteristics for a thread
// pool.
//
// See rust-lang-nursery/futures-rs#617
//
mod cpupool {
    use futures::future::{self, Executor};
    use futures::{task, Async};
    use futures_cpupool::*;
    use num_cpus;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;
    use std::sync::{mpsc, Arc};
    use test;

    #[bench]
    fn spawn_many(b: &mut test::Bencher) {
        let pool = CpuPool::new(num_cpus::get());

        let (tx, rx) = mpsc::sync_channel(10);
        let rem = Arc::new(AtomicUsize::new(0));

        b.iter(move || {
            rem.store(super::NUM_SPAWN, SeqCst);

            for _ in 0..super::NUM_SPAWN {
                let tx = tx.clone();
                let rem = rem.clone();

                pool.execute(future::lazy(move || {
                    if 1 == rem.fetch_sub(1, SeqCst) {
                        tx.send(()).unwrap();
                    }

                    Ok(())
                }))
                .ok()
                .unwrap();
            }

            let _ = rx.recv().unwrap();
        });
    }

    #[bench]
    fn yield_many(b: &mut test::Bencher) {
        let pool = CpuPool::new(num_cpus::get());
        let tasks = super::TASKS_PER_CPU * num_cpus::get();

        let (tx, rx) = mpsc::sync_channel(tasks);

        b.iter(move || {
            for _ in 0..tasks {
                let mut rem = super::NUM_YIELD;
                let tx = tx.clone();

                pool.execute(future::poll_fn(move || {
                    rem -= 1;

                    if rem == 0 {
                        tx.send(()).unwrap();
                        Ok(Async::Ready(()))
                    } else {
                        // Notify the current task
                        task::current().notify();

                        // Not ready
                        Ok(Async::NotReady)
                    }
                }))
                .ok()
                .unwrap();
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}
