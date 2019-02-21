extern crate env_logger;
extern crate futures;
extern crate tokio_threadpool;

use futures::sync::oneshot;
use futures::*;
use tokio_threadpool::*;

pub fn main() {
    let _ = ::env_logger::init();

    let pool = ThreadPool::new();
    let tx = pool.sender().clone();

    let res = oneshot::spawn(
        future::lazy(|| {
            println!("Running on the pool");
            Ok::<_, ()>("complete")
        }),
        &tx,
    );

    println!("Result: {:?}", res.wait());
}
