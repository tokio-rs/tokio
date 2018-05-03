#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio_core;
extern crate tokio_signal;

use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use futures::stream::Stream;
use futures::Future;
use tokio_core::reactor::{Core, Timeout};
use tokio_signal::unix::Signal;

#[test]
fn simple() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    lp.run(signal.into_future()).ok().unwrap();
}

#[test]
fn notify_both() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal1 = lp.run(Signal::new(libc::SIGUSR2, &handle)).unwrap();
    let signal2 = lp.run(Signal::new(libc::SIGUSR2, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR2), 0);
    }
    lp.run(signal1.into_future().join(signal2.into_future()))
        .ok()
        .unwrap();
}

#[test]
fn drop_then_get_a_signal() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    drop(signal);
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    let timeout = Timeout::new(Duration::from_millis(1), &lp.handle()).unwrap();
    lp.run(timeout).unwrap();
}

#[test]
fn twice() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    let (num, signal) = lp.run(signal.into_future()).ok().unwrap();
    assert_eq!(num, Some(libc::SIGUSR1));
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    lp.run(signal.into_future()).ok().unwrap();
}

#[test]
fn multi_loop() {
    // An "ordinary" (non-future) channel
    let (sender, receiver) = channel();
    // Run multiple times, to make sure there are no race conditions
    for _ in 0..10 {
        // Run multiple event loops, each one in its own thread
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let sender = sender.clone();
                thread::spawn(move || {
                    let mut lp = Core::new().unwrap();
                    let handle = lp.handle();
                    let signal = lp.run(Signal::new(libc::SIGHUP, &handle)).unwrap();
                    sender.send(()).unwrap();
                    lp.run(signal.into_future()).ok().unwrap();
                })
            })
            .collect();
        // Wait for them to declare they're ready
        for &_ in threads.iter() {
            receiver.recv().unwrap();
        }
        // Send a signal
        unsafe {
            assert_eq!(libc::kill(libc::getpid(), libc::SIGHUP), 0);
        }
        // Make sure the threads terminated correctly
        for t in threads {
            t.join().unwrap();
        }
    }
}
