#![feature(test)]

extern crate futures;
extern crate tokio_threadpool;
extern crate test;

const NUM_SPAWN: usize = 10_000;
const NUM_YIELD: usize = 1_000;
const TASKS_PER_CPU: usize = 50;

// TODO : update comment
//
// In this case, CPU pool completes the benchmark faster, but this is due to how
// CpuPool currently behaves, starving other futures. This completes the
// benchmark quickly but results in poor runtime characteristics for a thread
// pool.
//
// See alexcrichton/futures-rs#617
//
mod cpupool {
    use futures::Future;
    use futures::future::{self, FutureResult};
    use futures::sync::oneshot::spawn;
    use tokio_threadpool::*;
    use test;
    use std::sync::{mpsc, Arc};
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;

    #[bench]
    fn spawn_many(b: &mut test::Bencher) {
        let pool = ThreadPool::new();

        let (tx, rx) = mpsc::sync_channel(10);
        let rem = Arc::new(AtomicUsize::new(0));

        b.iter(move || {
            rem.store(super::NUM_SPAWN, SeqCst);

            for _ in 0..super::NUM_SPAWN {
                let tx = tx.clone();
                let rem = rem.clone();

                let fut = future::lazy(move || -> FutureResult<(), ()> {
                    if 1 == rem.fetch_sub(1, SeqCst) {
                        tx.send(()).unwrap();
                    }

                    future::ok(())
                });
                let pool_sender = pool.sender().clone();
                spawn(fut, &pool_sender).wait().unwrap();
            }

            let _ = rx.recv().unwrap();
        });
    }

    /* TODO: fix bench

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
                })).ok().unwrap();
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
    */
}

/* TODO do we need this bench?

mod us {
    use futures::{task, Async};
    use futures::future::{self, Executor};
    use futures_pool::*;
    use num_cpus;
    use test;
    use std::sync::{mpsc, Arc};
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;

    #[bench]
    fn spawn_many(b: &mut test::Bencher) {
        let (sched_tx, _scheduler) = Pool::new();

        let (tx, rx) = mpsc::sync_channel(10);
        let rem = Arc::new(AtomicUsize::new(0));

        b.iter(move || {
            rem.store(super::NUM_SPAWN, SeqCst);

            for _ in 0..super::NUM_SPAWN {
                let tx = tx.clone();
                let rem = rem.clone();

                sched_tx.execute(future::lazy(move || {
                    if 1 == rem.fetch_sub(1, SeqCst) {
                        tx.send(()).unwrap();
                    }

                    Ok(())
                })).ok().unwrap();
            }

            let _ = rx.recv().unwrap();
        });
    }

    #[bench]
    fn yield_many(b: &mut test::Bencher) {
        let (sched_tx, _scheduler) = Pool::new();
        let tasks = super::TASKS_PER_CPU * num_cpus::get();

        let (tx, rx) = mpsc::sync_channel(tasks);

        b.iter(move || {
            for _ in 0..tasks {
                let mut rem = super::NUM_YIELD;
                let tx = tx.clone();

                sched_tx.execute(future::poll_fn(move || {
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
                })).ok().unwrap();
            }

            for _ in 0..tasks {
                let _ = rx.recv().unwrap();
            }
        });
    }
}
*/
