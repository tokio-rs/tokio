#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};
use tokio::net::lookup_host;

#[tokio::test]
async fn resolve_dns() -> io::Result<()> {
    let host = lookup_host("localhost:3000").await?;
    let expected = if host.is_ipv4() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3000)
    } else {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 3000)
    };
    assert_eq!(host, expected);

    Ok(())
}


#[tokio::test]
async fn unknown_host_errors() -> io::Result<()> {
    let err = lookup_host("localhhost:3000").await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Other);

    Ok(())
}

#[tokio::test]
async fn resolve_dns_multiple() -> io::Result<()> {
    let host = lookup_host("localhost:3000").next_addr().await?;
    let host = match host {
        Some(host) => host,
        None => panic!("DNS resolution broke"),
    };

    let expected = if host.is_ipv4() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3000)
    } else {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 3000)
    };
    assert_eq!(host, expected);

    Ok(())
}
