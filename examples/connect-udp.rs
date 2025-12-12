//! An example of hooking up stdin/stdout to a UDP stream.
//!
//! This example will connect to a socket address specified in the argument list
//! and then forward all data read on stdin to the server, printing out all data
//! received on stdout. Each line entered on stdin will be translated to a UDP
//! packet which is then sent to the remote address.
//!
//! Note that this is not currently optimized for performance, especially
//! around buffer management. Rather it's intended to show an example of
//! working with a client.
//!
//! This example can be quite useful when interacting with the other examples in
//! this repository! Many of them recommend running this as a simple "hook up
//! stdin/stdout to a server" to get up and running.

#![warn(rust_2018_idioms)]

use tokio::io::{stdin, stdout};
use tokio::net::UdpSocket;
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};

use bytes::Bytes;
use futures::{Sink, SinkExt, Stream, StreamExt};
use std::env;
use std::error::Error;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse what address we're going to connect to
    let args = env::args().skip(1).collect::<Vec<_>>();
    let addr = args
        .first()
        .ok_or("this program requires at least one argument")?;
    let addr = addr.parse::<SocketAddr>()?;

    let stdin = FramedRead::new(stdin(), BytesCodec::new());
    let stdin = stdin.map(|i| i.map(|bytes| bytes.freeze()));
    let stdout = FramedWrite::new(stdout(), BytesCodec::new());

    connect(&addr, stdin, stdout).await?;

    Ok(())
}

pub async fn connect(
    addr: &SocketAddr,
    stdin: impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
    stdout: impl Sink<Bytes, Error = std::io::Error> + Unpin,
) -> Result<(), Box<dyn Error>> {
    // We'll bind our UDP socket to a local IP/port, but for now we
    // basically let the OS pick both of those.
    let bind_addr = if addr.ip().is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };

    let socket = UdpSocket::bind(&bind_addr).await?;
    socket.connect(addr).await?;

    tokio::try_join!(send(stdin, &socket), recv(stdout, &socket))?;

    Ok(())
}

async fn send(
    mut stdin: impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
    writer: &UdpSocket,
) -> Result<(), std::io::Error> {
    while let Some(item) = stdin.next().await {
        let buf = item?;
        writer.send(&buf[..]).await?;
    }

    Ok(())
}

async fn recv(
    mut stdout: impl Sink<Bytes, Error = std::io::Error> + Unpin,
    reader: &UdpSocket,
) -> Result<(), std::io::Error> {
    loop {
        let mut buf = vec![0; 1024];
        let n = reader.recv(&mut buf[..]).await?;

        if n > 0 {
            stdout.send(Bytes::copy_from_slice(&buf[..n])).await?;
        }
    }
}
