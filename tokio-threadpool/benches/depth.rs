#![feature(test)]

extern crate futures;
extern crate tokio_threadpool;
extern crate test;

const ITER: usize = 20_000;

mod threadpool {
    use futures::future::{self, Executor};
    use tokio_threadpool::*;
    use test;
    use std::sync::mpsc;

    #[bench]
    fn chained_spawn(b: &mut test::Bencher) {
        let pool = ThreadPool::new();
        let tx = pool.sender().clone();

        fn spawn(tx: Sender, res_tx: mpsc::Sender<()>, n: usize) {
            if n == 0 {
                res_tx.send(()).unwrap();
            } else {
                let tx2 = tx.clone();
                tx.execute(future::lazy(move || {
                    spawn(tx2, res_tx, n - 1);
                    Ok(())
                })).ok().unwrap();
            }
        }

        b.iter(move || {
            let (res_tx, res_rx) = mpsc::channel();

            spawn(tx.clone(), res_tx.clone(), super::ITER);
            res_rx.recv().unwrap();
        });
    }
}

/* TODO do we need this bench?

mod us {
    use futures::future::{self, Executor};
    use futures_pool::*;
    use test;
    use std::sync::mpsc;

    #[bench]
    fn chained_spawn(b: &mut test::Bencher) {
        let (sched_tx, _scheduler) = Pool::new();

        fn spawn(sched_tx: Sender, res_tx: mpsc::Sender<()>, n: usize) {
            if n == 0 {
                res_tx.send(()).unwrap();
            } else {
                let sched_tx2 = sched_tx.clone();
                sched_tx.execute(future::lazy(move || {
                    spawn(sched_tx2, res_tx, n - 1);
                    Ok(())
                })).ok().unwrap();
            }
        }

        b.iter(move || {
            let (res_tx, res_rx) = mpsc::channel();

            spawn(sched_tx.clone(), res_tx, super::ITER);
            res_rx.recv().unwrap();
        });
    }
}

*/
