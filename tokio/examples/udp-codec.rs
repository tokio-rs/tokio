//! This example leverages `BytesCodec` to create a UDP client and server which
//! speak a custom protocol.
//!
//! Here we're using the codec from tokio-io to convert a UDP socket to a stream of
//! client messages. These messages are then processed and returned back as a
//! new message with a new destination. Overall, we then use this to construct a
//! "ping pong" pair where two sockets are sending messages back and forth.

#![deny(warnings)]

extern crate env_logger;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;

use std::net::SocketAddr;

use tokio::net::{UdpFramed, UdpSocket};
use tokio::prelude::*;
use tokio_codec::BytesCodec;

fn main() -> Result<(), Box<std::error::Error>> {
    let _ = env_logger::init();

    let addr: SocketAddr = "127.0.0.1:0".parse()?;

    // Bind both our sockets and then figure out what ports we got.
    let a = UdpSocket::bind(&addr)?;
    let b = UdpSocket::bind(&addr)?;
    let b_addr = b.local_addr()?;

    // We're parsing each socket with the `BytesCodec` included in `tokio_io`, and then we
    // `split` each codec into the sink/stream halves.
    let (a_sink, a_stream) = UdpFramed::new(a, BytesCodec::new()).split();
    let (b_sink, b_stream) = UdpFramed::new(b, BytesCodec::new()).split();

    // Start off by sending a ping from a to b, afterwards we just print out
    // what they send us and continually send pings
    // let pings = stream::iter((0..5).map(Ok));
    let a = a_sink.send(("PING".into(), b_addr)).and_then(|a_sink| {
        let mut i = 0;
        let a_stream = a_stream.take(4).map(move |(msg, addr)| {
            i += 1;
            println!("[a] recv: {}", String::from_utf8_lossy(&msg));
            (format!("PING {}", i).into(), addr)
        });
        a_sink.send_all(a_stream)
    });

    // The second client we have will receive the pings from `a` and then send
    // back pongs.
    let b_stream = b_stream.map(|(msg, addr)| {
        println!("[b] recv: {}", String::from_utf8_lossy(&msg));
        ("PONG".into(), addr)
    });
    let b = b_sink.send_all(b_stream);

    // Spawn the sender of pongs and then wait for our pinger to finish.
    tokio::run({
        b.join(a)
            .map(|_| ())
            .map_err(|e| println!("error = {:?}", e))
    });
    Ok(())
}
