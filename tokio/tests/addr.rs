use tokio::net;
use std::{io, net::SocketAddr};

#[tokio::test]
async fn resolve_dns() -> io::Result<()> {
    let mut fut = net::lookup_host("localhost:3000");
    let host = fut.next_addr().await?;

    Ok(())
}