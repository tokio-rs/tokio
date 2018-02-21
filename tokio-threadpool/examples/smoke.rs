extern crate futures;
extern crate tokio_threadpool;
extern crate tokio_timer;
extern crate env_logger;

use tokio_threadpool::*;
use tokio_timer::Timer;

use futures::*;
use futures::sync::oneshot::spawn;

use std::thread;
use std::time::Duration;

pub fn main() {
    let _ = ::env_logger::init();

    let timer = Timer::default();
    {
        let pool = ThreadPool::new();
        let tx = pool.sender().clone();

        let fut = timer.interval(Duration::from_millis(300))
            .for_each(|_| {
                println!("~~~~~ Hello ~~~");
                Ok(())
            })
            .map_err(|_| unimplemented!());

        spawn(fut, &tx).wait().unwrap();
    }

    thread::sleep(Duration::from_millis(100));
}
