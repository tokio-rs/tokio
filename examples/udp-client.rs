//! A UDP client that just sends everything it gets via `stdio` in a single datagram, and then
//! waits for a reply.
//!
//! For the reasons of simplicity data from `stdio` is read until `EOF` in a blocking manner.
//!
//! You can test this out by running an echo server:
//!
//! ```
//!     $ cargo run --example echo-udp -- 127.0.0.1:8080
//! ```
//!
//! and running the client in another terminal:
//!
//! ```
//!     $ cargo run --example udp-client
//! ```
//!
//! You can optionally provide any custom endpoint address for the client:
//!
//! ```
//!     $ cargo run --example udp-client -- 127.0.0.1:8080
//! ```
//!
//! Don't forget to pass `EOF` to the standard input of the client!
//!
//! Please mind that since the UDP protocol doesn't have any capabilities to detect a broken
//! connection the server needs to be run first, otherwise the client will block forever.

extern crate futures;
extern crate tokio;

use std::env;
use std::io::stdin;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::prelude::*;

fn get_stdin_data() -> Result<Vec<u8>, Box<std::error::Error>> {
    let mut buf = Vec::new();
    stdin().read_to_end(&mut buf)?;
    Ok(buf)
}

fn main() -> Result<(), Box<std::error::Error>> {
    let remote_addr: SocketAddr = env::args()
        .nth(1)
        .unwrap_or("127.0.0.1:8080".into())
        .parse()?;
    // We use port 0 to let the operating system allocate an available port for us.
    let local_addr: SocketAddr = if remote_addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()?;
    let socket = UdpSocket::bind(&local_addr)?;
    const MAX_DATAGRAM_SIZE: usize = 65_507;
    socket
        .send_dgram(get_stdin_data()?, &remote_addr)
        .and_then(|(socket, _)| socket.recv_dgram(vec![0u8; MAX_DATAGRAM_SIZE]))
        .map(|(_, data, len, _)| {
            println!(
                "Received {} bytes:\n{}",
                len,
                String::from_utf8_lossy(&data[..len])
            )
        })
        .wait()?;
    Ok(())
}
