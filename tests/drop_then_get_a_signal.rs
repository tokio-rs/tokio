#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use std::time::Duration;

use tokio_core::reactor::{Core, Timeout};
use tokio_signal::unix::Signal;

#[test]
fn drop_then_get_a_signal() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::with_handle(
        libc::SIGUSR1,
        &handle.new_tokio_handle(),
    )).unwrap();
    drop(signal);
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    let timeout = Timeout::new(Duration::from_millis(1), &lp.handle()).unwrap();
    lp.run(timeout).unwrap();
}
