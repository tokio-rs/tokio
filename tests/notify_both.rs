#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use futures::stream::Stream;
use futures::Future;
use tokio_core::reactor::Core;
use tokio_signal::unix::Signal;

#[test]
fn notify_both() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal1 = lp.run(Signal::with_handle(
        libc::SIGUSR2,
        &handle.new_tokio_handle(),
    )).unwrap();
    let signal2 = lp.run(Signal::with_handle(
        libc::SIGUSR2,
        &handle.new_tokio_handle(),
    )).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR2), 0);
    }
    lp.run(signal1.into_future().join(signal2.into_future()))
        .ok()
        .unwrap();
}
