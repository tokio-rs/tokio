#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use std::sync::mpsc::channel;
use std::thread;

use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_signal::unix::Signal;

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
                    let signal = lp.run(Signal::new(libc::SIGHUP)).unwrap();
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
