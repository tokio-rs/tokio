extern crate tokio;

use std::thread;
use std::time::{Duration, Instant};
use std::sync::mpsc;

use tokio::reactor::Reactor;

#[test]
fn works() {
    let mut r = Reactor::new().unwrap();
    r.handle().wakeup();
    r.turn(None);

    let now = Instant::now();
    let mut n = 0;
    while now.elapsed() < Duration::from_millis(10) {
        n += 1;
        r.turn(Some(Duration::from_millis(10)));
    }
    assert!(n < 5);
}

#[test]
fn wakes() {
    const N: usize = 1_000;

    let mut r = Reactor::new().unwrap();
    let handle = r.handle();
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        for _ in 0..N {
            rx.recv().unwrap();
            handle.wakeup();
        }
    });

    for _ in 0..N {
        tx.send(()).unwrap();
        r.turn(None);
    }
    t.join().unwrap();
}
