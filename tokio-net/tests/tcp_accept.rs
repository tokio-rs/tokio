#![warn(rust_2018_idioms)]

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_test::assert_ok;

use std::net::{IpAddr, SocketAddr};

macro_rules! test_accept {
    ($(($ident:ident, $target:expr),)*) => {
        $(
            #[tokio::test]
            async fn $ident() {
                let mut listener = assert_ok!(TcpListener::bind($target).await);
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
        )*
    }
}

test_accept! {
    (ip_str, "127.0.0.1:0"),
    (host_str, "localhost:0"),
    (socket_addr, "127.0.0.1:0".parse::<SocketAddr>().unwrap()),
    (str_port_tuple, ("127.0.0.1", 0)),
    (ip_port_tuple, ("127.0.0.1".parse::<IpAddr>().unwrap(), 0)),
}
