//! An UDP echo server that just sends back everything that it receives.
//!
//! If you're on Unix you can test this out by in one terminal executing:
//!
//!     cargo run --example echo-udp
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect-udp 127.0.0.1:8080
//!
//! Each line you type in to the `connect-udp` terminal should be echo'd back to you!
//!
//! # Binding to all interfaces
//!
//! By default this example binds to `127.0.0.1` so it is only reachable
//! from the local machine.
//!
//! To listen on all interfaces instead:
//!
//! ```sh
//! cargo run --example echo-udp -- 0.0.0.0:8080
//! ```
//!
//! Binding to `0.0.0.0` exposes the server on all network interfaces.
//! Only do this in trusted network environments.
//!
//! On multi-homed systems, a UDP socket bound to a wildcard address
//! (`0.0.0.0` or `::`) cannot always send replies from the same local IP
//! that received the packet. Replies may therefore originate from a
//! different address than the client targeted. See [this Cloudflare blog
//! post][udp-blog] for more details.
//!
//! [udp-blog]: https://blog.cloudflare.com/everything-you-ever-wanted-to-know-about-udp-sockets-but-were-afraid-to-ask-part-1

#![warn(rust_2018_idioms)]

use std::error::Error;
use std::net::SocketAddr;
use std::{env, io};
use tokio::net::UdpSocket;

struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl Server {
    async fn run(self) -> Result<(), io::Error> {
        let Server {
            socket,
            mut buf,
            mut to_send,
        } = self;

        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = to_send {
                let amt = socket.send_to(&buf[..size], &peer).await?;

                println!("Echoed {amt}/{size} bytes to {peer}");
            }

            // If we're here then `to_send` is `None`, so we take a look for the
            // next message we're going to echo back.
            to_send = Some(socket.recv_from(&mut buf).await?);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let socket = UdpSocket::bind(&addr).await?;
    println!("Listening on: {}", socket.local_addr()?);

    let server = Server {
        socket,
        buf: vec![0; 1024],
        to_send: None,
    };

    // This starts the server task.
    server.run().await?;

    Ok(())
}
