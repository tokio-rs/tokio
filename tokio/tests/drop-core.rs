extern crate futures;
extern crate tokio;

use std::net;
use std::thread;

use futures::future;
use futures::prelude::*;
use futures::sync::oneshot;
use tokio::net::TcpListener;
use tokio::reactor::Reactor;

#[test]
fn tcp_doesnt_block() {
    let core = Reactor::new().unwrap();
    let handle = core.handle();
    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let listener = TcpListener::from_std(listener, &handle).unwrap();
    drop(core);
    assert!(listener.incoming().wait().next().unwrap().is_err());
}

#[test]
fn drop_wakes() {
    let core = Reactor::new().unwrap();
    let handle = core.handle();
    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let listener = TcpListener::from_std(listener, &handle).unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let t = thread::spawn(move || {
        let incoming = listener.incoming();
        let new_socket = incoming.into_future().map_err(|_| ());
        let drop_tx = future::lazy(|| {
            drop(tx);
            future::ok(())
        });
        assert!(new_socket.join(drop_tx).wait().is_err());
    });
    drop(rx.wait());
    drop(core);
    t.join().unwrap();
}
