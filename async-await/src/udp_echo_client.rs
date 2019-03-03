#![feature(await_macro, async_await)]

use tokio::net::UdpSocket;
use tokio::prelude::*;

use std::io;

const MESSAGES: &[&str] = &["hello", "world", "one two three"];

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = std::env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse().unwrap();
    let bind_addr = "127.0.0.1:0".parse().unwrap();
    let mut socket = UdpSocket::bind(&bind_addr)?;
    // Connect to server address.
    socket.connect(&addr)?;

    // Buffer to read into
    let mut buf = [0; 128];

    for msg in MESSAGES {
        println!(" > write = {:?}", msg);

        // Write the message to the server
        await!(socket.send_async(msg.as_bytes()))?;

        // Read the message back from the server
        await!(socket.recv_async(&mut buf[..msg.len()]))?;

        assert_eq!(&buf[..msg.len()], msg.as_bytes());
    }

    Ok(())
}
