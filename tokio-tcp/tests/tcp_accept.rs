#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_test::assert_ok;

#[tokio::test]
async fn accept() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = assert_ok!(TcpListener::bind(&addr));
    let addr = listener.local_addr().unwrap();

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (socket, _) = assert_ok!(listener.accept().await);
        assert_ok!(tx.send(socket));
    });

    let cli = assert_ok!(TcpStream::connect(&addr).await);
    let srv = assert_ok!(rx.await);

    assert_eq!(cli.local_addr().unwrap(), srv.peer_addr().unwrap());
}
