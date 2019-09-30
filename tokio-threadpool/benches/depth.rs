#![feature(test)]

extern crate futures;
extern crate futures_cpupool;
extern crate num_cpus;
extern crate test;
extern crate tokio_threadpool;

const ITER: usize = 20_000;

mod us {
    use futures::future;
    use std::sync::mpsc;
    use test;
    use tokio_threadpool::*;

    #[bench]
    fn chained_spawn(b: &mut test::Bencher) {
        let threadpool = ThreadPool::new();

        fn spawn(pool_tx: Sender, res_tx: mpsc::Sender<()>, n: usize) {
            if n == 0 {
                res_tx.send(()).unwrap();
            } else {
                let pool_tx2 = pool_tx.clone();
                pool_tx
                    .spawn(future::lazy(move || {
                        spawn(pool_tx2, res_tx, n - 1);
                        Ok(())
                    }))
                    .unwrap();
            }
        }

        b.iter(move || {
            let (res_tx, res_rx) = mpsc::channel();

            spawn(threadpool.sender().clone(), res_tx, super::ITER);
            res_rx.recv().unwrap();
        });
    }
}

mod cpupool {
    use futures::future::{self, Executor};
    use futures_cpupool::*;
    use num_cpus;
    use std::sync::mpsc;
    use test;

    #[bench]
    fn chained_spawn(b: &mut test::Bencher) {
        let pool = CpuPool::new(num_cpus::get());

        fn spawn(pool: CpuPool, res_tx: mpsc::Sender<()>, n: usize) {
            if n == 0 {
                res_tx.send(()).unwrap();
            } else {
                let pool2 = pool.clone();
                pool.execute(future::lazy(move || {
                    spawn(pool2, res_tx, n - 1);
                    Ok(())
                }))
                .ok()
                .unwrap();
            }
        }

        b.iter(move || {
            let (res_tx, res_rx) = mpsc::channel();

            spawn(pool.clone(), res_tx, super::ITER);
            res_rx.recv().unwrap();
        });
    }
}
