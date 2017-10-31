extern crate futures;
extern crate tokio;

use std::net;
use std::io;

use futures::future;
use futures::prelude::*;
use futures::thread;
use tokio::net::TcpListener;
use tokio::reactor::Reactor;

#[test]
fn drop_core_cancels() {
    let reactor = Reactor::new().unwrap();
    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = reactor.handle().clone();
    let mut listener = TcpListener::from_listener(listener, &addr, handle).unwrap();

    let mut reactor = Some(reactor);
    thread::block_until(future::poll_fn(|| {
        match listener.accept() {
            Ok(_) => return Ok(Async::Ready(())),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        drop(reactor.take());
        Ok(Async::NotReady)
    })).unwrap_err();
}
