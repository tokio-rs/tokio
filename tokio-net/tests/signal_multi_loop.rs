#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]

mod support;
use support::*;

use std::sync::mpsc::channel;
use std::thread;

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
                    let mut rt = CurrentThreadRuntime::new().unwrap();
                    let signal = signal(SignalKind::hangup()).unwrap();
                    sender.send(()).unwrap();
                    let _ = run_with_timeout(&mut rt, signal.into_future());
                })
            })
            .collect();
        // Wait for them to declare they're ready
        for &_ in threads.iter() {
            receiver.recv().unwrap();
        }
        // Send a signal
        send_signal(libc::SIGHUP);
        // Make sure the threads terminated correctly
        for t in threads {
            t.join().unwrap();
        }
    }
}
