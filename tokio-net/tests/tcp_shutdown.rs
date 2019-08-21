#![warn(rust_2018_idioms)]

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio_test::assert_ok;

#[tokio::test]
async fn shutdown() {
    let addr = assert_ok!("127.0.0.1:0".parse());
    let mut srv = assert_ok!(TcpListener::bind(&addr));
    let addr = assert_ok!(srv.local_addr());

    tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);

        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);

        let mut buf = [0; 1];
        let n = assert_ok!(stream.read(&mut buf).await);
        assert_eq!(n, 0);
    });

    let (stream, _) = assert_ok!(srv.accept().await);
    let (mut rd, mut wr) = stream.split();

    let n = assert_ok!(rd.copy(&mut wr).await);
    assert_eq!(n, 0);
}
