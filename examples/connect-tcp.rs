//! An example of hooking up stdin/stdout to a TCP stream.
//!
//! This example will connect to a socket address specified in the argument list
//! and then forward all data read on stdin to the server, printing out all data
//! received on stdout. Each line entered on stdin will be translated to a TCP
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
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};

use bytes::Bytes;
use futures::{future, Sink, SinkExt, Stream, StreamExt};
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
    mut stdin: impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
    mut stdout: impl Sink<Bytes, Error = std::io::Error> + Unpin,
) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(addr).await?;
    let (r, w) = stream.split();
    let mut sink = FramedWrite::new(w, BytesCodec::new());
    // filter map Result<BytesMut, Error> stream into just a Bytes stream to match stdout Sink
    // on the event of an Error, log the error and end the stream
    let mut stream = FramedRead::new(r, BytesCodec::new())
        .filter_map(|i| match i {
            //BytesMut into Bytes
            Ok(i) => future::ready(Some(i.freeze())),
            Err(e) => {
                eprintln!("failed to read from socket; error={e}");
                future::ready(None)
            }
        })
        .map(Ok);

    match future::join(sink.send_all(&mut stdin), stdout.send_all(&mut stream)).await {
        (Err(e), _) | (_, Err(e)) => Err(e.into()),
        _ => Ok(()),
    }
}
