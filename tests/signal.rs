#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use futures::stream::Stream;
use futures::{Future, IntoFuture};
use tokio_signal::unix::Signal;

#[test]
fn tokio_simple() {
    tokio::run(
        Signal::new(libc::SIGUSR1)
            .into_future()
            .and_then(|signal| {
                unsafe {
                    assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
                }
                signal.into_future().map(|_| ()).map_err(|(err, _)| err)
            })
            .map_err(|err| panic!("{}", err)),
    )
}
