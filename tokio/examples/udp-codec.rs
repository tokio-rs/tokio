//! This example leverages `BytesCodec` to create a UDP client and server which
//! speak a custom protocol.
//!
//! Here we're using the codec from `tokio-codec` to convert a UDP socket to a stream of
//! client messages. These messages are then processed and returned back as a
//! new message with a new destination. Overall, we then use this to construct a
//! "ping pong" pair where two sockets are sending messages back and forth.

#![feature(async_await)]
#![deny(warnings, rust_2018_idioms)]

use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io;
use tokio::net::UdpSocket;
use tokio::util::FutureExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = env_logger::init();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:0".to_string());
    let addr = addr.parse::<SocketAddr>()?;

    // Bind both our sockets and then figure out what ports we got.
    let mut a = UdpSocket::bind(&addr)?;
    let mut b = UdpSocket::bind(&addr)?;
    let b_addr = b.local_addr()?;

    // Start off by sending a ping from a to b, afterwards we just print out
    // what they send us and continually send pings
    let a = ping(&mut a, b_addr);

    // The second client we have will receive the pings from `a` and then send
    // back pongs.
    let b = pong(&mut b);

    // Run both futures simultaneously of `a` and `b` sending messages back and forth.
    match futures::future::try_join(a, b).await {
        Err(e) => println!("an error occured; error = {:?}", e),
        _ => println!("done!"),
    }

    Ok(())
}

async fn ping(socket: &mut UdpSocket, b_addr: SocketAddr) -> Result<(), io::Error> {
    socket.send_to(b"PING", &b_addr).await?;

    for _ in 0..4usize {
        let mut buffer = [0u8; 255];

        let (bytes_read, addr) = socket.recv_from(&mut buffer).await?;

        println!(
            "[a] recv: {}",
            String::from_utf8_lossy(&buffer[..bytes_read])
        );

        socket.send_to(b"PING", &addr).await?;
    }

    Ok(())
}

async fn pong(socket: &mut UdpSocket) -> Result<(), io::Error> {
    let mut buffer = [0u8; 255];

    while let Ok(Ok((bytes_read, addr))) = socket
        .recv_from(&mut buffer)
        .timeout(Duration::from_millis(200))
        .await
    {
        println!(
            "[b] recv: {}",
            String::from_utf8_lossy(&buffer[..bytes_read])
        );

        socket.send_to(b"PONG", &addr).await?;
    }

    Ok(())
}
