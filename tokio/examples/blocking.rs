//! An example of using blocking funcion annotation.
//!
//! This example will create 8 "heavy computation" blocking futures and 8
//! non-blocking futures with 4 threads core threads in runtime.
//! Each non-blocking future will print it's id and return immideatly.
//! Each blocking future will print it's id on start, sleep for 1000 ms, print
//! it's id and return.
//!
//! Note how non-blocking threads are executed before blocking threads finish
//! their task.

extern crate tokio;
extern crate tokio_threadpool;

use std::thread;
use std::time::Duration;
use tokio::prelude::*;
use tokio::runtime::Builder;
use tokio_threadpool::blocking;

/// This future blocks it's poll method for 1000 ms.
struct BlockingFuture {
    value: i32,
}

impl Future for BlockingFuture {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        println!("Blocking begin: {}!", self.value);
        // Try replacing this part with commnted code
        blocking(|| {
            println!("Blocking part annotated: {}!", self.value);
            thread::sleep(Duration::from_millis(1000));
            println!("Blocking done annotated: {}!", self.value);
        })
        .map_err(|err| panic!("Error in blocing block: {:?}", err))
        // println!("Blocking part annotated: {}!", self.value);
        // thread::sleep(Duration::from_millis(1000));
        // println!("Blocking done annotated: {}!", self.value);
        // Ok(Async::Ready(()))
    }
}

/// This future returns immideatly.
struct NonBlockingFuture {
    value: i32,
}

impl Future for NonBlockingFuture {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        println!("Non-blocking done: {}!", self.value);
        Ok(Async::Ready(()))
    }
}

/// This future spawns child futures.
struct SpawningFuture;

impl Future for SpawningFuture {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        for i in 0..8 {
            let blocking_future = BlockingFuture { value: i };

            tokio::spawn(blocking_future);
        }
        for i in 0..8 {
            let non_blocking_future = NonBlockingFuture { value: i };
            tokio::spawn(non_blocking_future);
        }
        Ok(Async::Ready(()))
    }
}

fn main() {
    let spawning_future = SpawningFuture;

    let runtime = Builder::new().core_threads(4).build().unwrap();
    runtime.block_on_all(spawning_future).unwrap();
}
