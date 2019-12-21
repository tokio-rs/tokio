use tokio::net;
use tokio_test::assert_ok;

use std::net::SocketAddr;

#[tokio::test]
async fn lookup_socket_addr() {
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

    let actual = assert_ok!(net::lookup_host(addr).await).collect::<Vec<_>>();
    assert_eq!(vec![addr], actual);
}

#[tokio::test]
async fn lookup_str_socket_addr() {
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

    let actual = assert_ok!(net::lookup_host("127.0.0.1:8000").await).collect::<Vec<_>>();
    assert_eq!(vec![addr], actual);
}
