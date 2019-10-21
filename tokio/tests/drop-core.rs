#![warn(rust_2018_idioms)]
#![cfg(feature = "default")]

use tokio::net::TcpListener;
use tokio_net::driver::{self, Reactor};
use tokio_test::{assert_err, assert_pending, assert_ready, task};

#[test]
fn tcp_doesnt_block() {
    let reactor = Reactor::new().unwrap();
    let handle = reactor.handle();

    // Set the current reactor for this thread
    let _reactor_guard = driver::set_default(&handle);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut listener = TcpListener::from_std(listener).unwrap();
    drop(reactor);

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_ready!(task.poll());
}

#[test]
fn drop_wakes() {
    let reactor = Reactor::new().unwrap();
    let handle = reactor.handle();

    // Set the current reactor for this thread
    let _reactor_guard = driver::set_default(&handle);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut listener = TcpListener::from_std(listener).unwrap();

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_pending!(task.poll());

    drop(reactor);

    assert!(task.is_woken());
    assert_ready!(task.poll());
}
