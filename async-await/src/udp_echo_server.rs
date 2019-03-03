#![feature(await_macro, async_await)]

use tokio::net::UdpSocket;
use tokio::prelude::*;

#[tokio::main]
async fn main() {
    use std::env;

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse().unwrap();

    let mut socket = UdpSocket::bind(&addr).unwrap();

    let mut buf = [0u8; 1500];
    loop {
        let (len, peer_addr) = await!(socket.recv_from_async(&mut buf)).unwrap();
        await!(socket.send_to_async(&buf[..len], &peer_addr)).unwrap();
    }
}
