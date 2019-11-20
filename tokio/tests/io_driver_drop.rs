#![warn(rust_2018_idioms)]

use tokio::net::TcpListener;
use tokio::runtime;
use tokio_test::{assert_err, assert_pending, assert_ready, task};

#[test]
fn tcp_doesnt_block() {
    let rt = runtime::Builder::new().basic_scheduler().build().unwrap();

    let mut listener = rt.enter(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        TcpListener::from_std(listener).unwrap()
    });

    drop(rt);

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_ready!(task.poll());
}

#[test]
fn drop_wakes() {
    let rt = runtime::Builder::new().basic_scheduler().build().unwrap();

    let mut listener = rt.enter(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        TcpListener::from_std(listener).unwrap()
    });

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_pending!(task.poll());

    drop(rt);

    assert!(task.is_woken());
    assert_ready!(task.poll());
}
