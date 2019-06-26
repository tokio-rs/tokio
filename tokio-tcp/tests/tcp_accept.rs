#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_test::assert_ok;

#[tokio::test]
async fn accept() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = assert_ok!(TcpListener::bind(&addr));

    let (mut tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        loop {
            let (socket, _) = assert_ok!(listener.accept().await);
            assert_ok!(tx.try_send(socket));
        }
    });

    let cli = assert_ok!(TcpStream::connect(&addr).await);
    let srv = rx.next().await;

    assert_eq!(cli.local_addr(), srv.peer_addr());


    /*
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let (tx, rx) = channel();
    let client = srv
        .incoming()
        .map(move |t| {
            tx.send(()).unwrap();
            t
        })
        .into_future()
        .map_err(|e| e.0);
    assert!(rx.try_recv().is_err());
    let t = thread::spawn(move || net::TcpStream::connect(&addr).unwrap());

    let (mine, _remaining) = t!(client.wait());
    let mine = mine.unwrap();
    let theirs = t.join().unwrap();

    assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
    assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
    */
}
