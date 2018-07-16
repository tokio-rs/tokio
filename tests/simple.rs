#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio_core;
extern crate tokio_signal;

use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_signal::unix::Signal;

#[test]
fn simple() {
    let mut lp = Core::new().unwrap();
    let signal = lp.run(Signal::new(libc::SIGUSR1)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    lp.run(signal.into_future()).ok().unwrap();
}
