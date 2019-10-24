use std::{
    io::{Error, ErrorKind},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};
use tokio_net::resolve_host;

#[tokio::test]
async fn resolve_host_ok() -> Result<(), Error> {
    let addr = resolve_host("127.0.0.1:8000").await?;
    let expected = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000);
    assert_eq!(addr, expected);

    Ok(())
}

#[tokio::test]
async fn resolve_host_err() -> Result<(), Error> {
    let err = resolve_host("local-not-host:8000").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Other);

    Ok(())
}
