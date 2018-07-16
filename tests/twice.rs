#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_signal::unix::Signal;

#[test]
fn twice() {
    let mut lp = Core::new().unwrap();
    let signal = lp.run(Signal::new(libc::SIGUSR1)).unwrap();
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
