//! An example of hooking up stdin/stdout to either a TCP or UDP stream.
//!
//! This example will connect to a socket address specified in the argument list
//! and then forward all data read on stdin to the server, printing out all data
//! received on stdout. An optional `--udp` argument can be passed to specify
//! that the connection should be made over UDP instead of TCP, translating each
//! line entered on stdin to a UDP packet to be sent to the remote address.
//!
//! Note that this is not currently optimized for performance, especially
//! around buffer management. Rather it's intended to show an example of
//! working with a client.
//!
//! This example can be quite useful when interacting with the other examples in
//! this repository! Many of them recommend running this as a simple "hook up
//! stdin/stdout to a server" to get up and running.

#![warn(rust_2018_idioms)]

use tokio::io;
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{FramedRead, FramedWrite};

use futures::{SinkExt, Stream, StreamExt};
use std::env;
use std::error::Error;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        run().await.unwrap();
        tx.send(()).unwrap();
    });

    rx.await.map_err(Into::into)
}

// Currently, we need to spawn the initial future due to https://github.com/tokio-rs/tokio/issues/1356
async fn run() -> Result<(), Box<dyn Error>> {
    // Determine if we're going to run in TCP or UDP mode
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let tcp = match args.iter().position(|a| a == "--udp") {
        Some(i) => {
            args.remove(i);
            false
        }
        None => true,
    };

    // Parse what address we're going to connect to
    let addr = match args.first() {
        Some(addr) => addr,
        None => Err("this program requires at least one argument")?,
    };
    let addr = addr.parse::<SocketAddr>()?;

    let stdin = stdin();
    let stdout = FramedWrite::new(io::stdout(), codec::Bytes);

    if tcp {
        tcp::connect(&addr, stdin, stdout).await?;
    } else {
        udp::connect(&addr, stdin, stdout).await?;
    }

    Ok(())
}

// Temporary work around for stdin blocking the stream
fn stdin() -> impl Stream<Item = Result<Vec<u8>, io::Error>> + Unpin {
    let mut stdin = FramedRead::new(io::stdin(), codec::Bytes).map(Ok);

    let (mut tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        tx.send_all(&mut stdin).await.unwrap();
    });

    rx
}

mod tcp {
    use super::codec;
    use futures::{future, Sink, SinkExt, Stream, StreamExt};
    use std::{error::Error, io, net::SocketAddr};
    use tokio::net::TcpStream;
    use tokio_util::codec::{FramedRead, FramedWrite};

    pub async fn connect(
        addr: &SocketAddr,
        stdin: impl Stream<Item = Result<Vec<u8>, io::Error>> + Unpin,
        mut stdout: impl Sink<Vec<u8>, Error = io::Error> + Unpin,
    ) -> Result<(), Box<dyn Error>> {
        let mut stream = TcpStream::connect(addr).await?;
        let (r, w) = stream.split();
        let sink = FramedWrite::new(w, codec::Bytes);
        let mut stream = FramedRead::new(r, codec::Bytes)
            .filter_map(|i| match i {
                Ok(i) => future::ready(Some(i)),
                Err(e) => {
                    println!("failed to read from socket; error={}", e);
                    future::ready(None)
                }
            })
            .map(Ok);

        match future::join(stdin.forward(sink), stdout.send_all(&mut stream)).await {
            (Err(e), _) | (_, Err(e)) => Err(e.into()),
            _ => Ok(()),
        }
    }
}

mod udp {
    use futures::{future, Sink, SinkExt, Stream, StreamExt};
    use std::{error::Error, io, net::SocketAddr};
    use tokio::net::udp::{
        split::{UdpSocketRecvHalf, UdpSocketSendHalf},
        UdpSocket,
    };

    pub async fn connect(
        addr: &SocketAddr,
        stdin: impl Stream<Item = Result<Vec<u8>, io::Error>> + Unpin,
        stdout: impl Sink<Vec<u8>, Error = io::Error> + Unpin,
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
        let (mut r, mut w) = socket.split();

        future::try_join(send(stdin, &mut w), recv(stdout, &mut r)).await?;

        Ok(())
    }

    async fn send(
        mut stdin: impl Stream<Item = Result<Vec<u8>, io::Error>> + Unpin,
        writer: &mut UdpSocketSendHalf,
    ) -> Result<(), io::Error> {
        while let Some(item) = stdin.next().await {
            let buf = item?;
            writer.send(&buf[..]).await?;
        }

        Ok(())
    }

    async fn recv(
        mut stdout: impl Sink<Vec<u8>, Error = io::Error> + Unpin,
        reader: &mut UdpSocketRecvHalf,
    ) -> Result<(), io::Error> {
        loop {
            let mut buf = vec![0; 1024];
            let n = reader.recv(&mut buf[..]).await?;

            if n > 0 {
                stdout.send(buf).await?;
            }
        }
    }
}

mod codec {
    use bytes::{BufMut, BytesMut};
    use std::io;
    use tokio_util::codec::{Decoder, Encoder};

    /// A simple `Codec` implementation that just ships bytes around.
    ///
    /// This type is used for "framing" a TCP/UDP stream of bytes but it's really
    /// just a convenient method for us to work with streams/sinks for now.
    /// This'll just take any data read and interpret it as a "frame" and
    /// conversely just shove data into the output location without looking at
    /// it.
    pub struct Bytes;

    impl Decoder for Bytes {
        type Item = Vec<u8>;
        type Error = io::Error;

        fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<Vec<u8>>> {
            if buf.len() > 0 {
                let len = buf.len();
                Ok(Some(buf.split_to(len).into_iter().collect()))
            } else {
                Ok(None)
            }
        }
    }

    impl Encoder for Bytes {
        type Item = Vec<u8>;
        type Error = io::Error;

        fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> io::Result<()> {
            buf.put(&data[..]);
            Ok(())
        }
    }
}
