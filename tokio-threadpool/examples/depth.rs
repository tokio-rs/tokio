extern crate env_logger;
extern crate futures;
extern crate tokio_threadpool;

use futures::future::{self, Executor};
use tokio_threadpool::*;

use std::sync::mpsc;

const ITER: usize = 2_000_000;
// const ITER: usize = 30;

fn chained_spawn() {
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
            }))
            .ok()
            .unwrap();
        }
    }

    loop {
        println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
        let (res_tx, res_rx) = mpsc::channel();

        for _ in 0..10 {
            spawn(tx.clone(), res_tx.clone(), ITER);
        }

        for _ in 0..10 {
            res_rx.recv().unwrap();
        }
    }
}

pub fn main() {
    let _ = ::env_logger::init();
    chained_spawn();
}
