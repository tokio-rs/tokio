extern crate tokio;
extern crate tokio_io;
extern crate env_logger;

use tokio::prelude::*;
use tokio::timer::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn timer_with_runtime() {
    let _ = env_logger::init();

    let when = Instant::now() + Duration::from_millis(100);
    let (tx, rx) = mpsc::channel();

    tokio::run({
        Sleep::new(when)
            .map_err(|e| panic!("unexpected error; err={:?}", e))
            .and_then(move |_| {
                assert!(Instant::now() >= when);
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}
