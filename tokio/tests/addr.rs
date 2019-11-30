#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::{
    io,
    net::{IpAddr, Ipv6Addr, SocketAddr},
};
use tokio::net::lookup_host;

#[tokio::test]
async fn resolve_dns() -> io::Result<()> {
    let host = lookup_host("localhost:3000").next_addr().await;
    let host = host.expect("localhost:3000");
    let actual = host.expect("no error in getting host");

    let expected = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 3000);
    assert_eq!(actual, expected);

    Ok(())
}
