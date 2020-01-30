#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::net::{TcpListener, TcpStream};
use tokio_test::assert_ok;

use futures::join;

#[tokio::test]
async fn readiness() {
    let mut srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());
    let addr = format!("localhost:{}", addr.port());

    let server = async {
        assert_ok!(srv.accept().await);
    };

    let client = async {
        let mut client = assert_ok!(TcpStream::connect(addr).await);
        assert_ok!(client.read_ready().await);
        assert_ok!(client.write_ready().await);
    };

    join!(server, client);
}
