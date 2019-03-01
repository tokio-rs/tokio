extern crate env_logger;
extern crate futures;
extern crate tokio_threadpool;

extern crate tokio_trace_fmt;
extern crate tokio_trace;

use futures::sync::oneshot;
use futures::*;
use tokio_threadpool::*;

pub fn main() {
    // let _ = ::env_logger::init();
    let subscriber = tokio_trace_fmt::FmtSubscriber::builder().full().finish();

    tokio_trace::subscriber::with_default(subscriber, || {
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
    })
}
