#![feature(test)]

extern crate futures;
extern crate rand;
extern crate test;
extern crate threadpool;
extern crate tokio_threadpool;

const ITER: usize = 1_000;

mod blocking {
    use super::*;

    use futures::future::*;
    use tokio_threadpool::{blocking, Builder};

    #[bench]
    fn cpu_bound(b: &mut test::Bencher) {
        let pool = Builder::new().pool_size(2).max_blocking(20).build();

        b.iter(|| {
            let count_down = Arc::new(CountDown::new(::ITER));

            for _ in 0..::ITER {
                let count_down = count_down.clone();

                pool.spawn(lazy(move || {
                    poll_fn(|| blocking(|| perform_complex_computation()).map_err(|_| panic!()))
                        .and_then(move |_| {
                            // Do something with the value
                            count_down.dec();
                            Ok(())
                        })
                }));
            }

            count_down.wait();
        })
    }
}

mod message_passing {
    use super::*;

    use futures::future::*;
    use futures::sync::oneshot;
    use tokio_threadpool::Builder;

    #[bench]
    fn cpu_bound(b: &mut test::Bencher) {
        let pool = Builder::new().pool_size(2).max_blocking(20).build();

        let blocking = threadpool::ThreadPool::new(20);

        b.iter(|| {
            let count_down = Arc::new(CountDown::new(::ITER));

            for _ in 0..::ITER {
                let count_down = count_down.clone();
                let blocking = blocking.clone();

                pool.spawn(lazy(move || {
                    // Create a channel to receive the return value.
                    let (tx, rx) = oneshot::channel();

                    // Spawn a task on the blocking thread pool to process the
                    // computation.
                    blocking.execute(move || {
                        let res = perform_complex_computation();
                        tx.send(res).unwrap();
                    });

                    rx.and_then(move |_| {
                        count_down.dec();
                        Ok(())
                    })
                    .map_err(|_| panic!())
                }));
            }

            count_down.wait();
        })
    }
}

fn perform_complex_computation() -> usize {
    use rand::*;

    // Simulate a CPU heavy computation
    let mut rng = rand::thread_rng();
    rng.gen()
}

// Util for waiting until the tasks complete

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::*;
use std::sync::*;

struct CountDown {
    rem: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

impl CountDown {
    fn new(rem: usize) -> Self {
        CountDown {
            rem: AtomicUsize::new(rem),
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }

    fn dec(&self) {
        let prev = self.rem.fetch_sub(1, AcqRel);

        if prev != 1 {
            return;
        }

        let _lock = self.mutex.lock().unwrap();
        self.condvar.notify_all();
    }

    fn wait(&self) {
        let mut lock = self.mutex.lock().unwrap();

        loop {
            if self.rem.load(Acquire) == 0 {
                return;
            }

            lock = self.condvar.wait(lock).unwrap();
        }
    }
}
