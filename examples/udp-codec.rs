//! This is a basic example of leveraging `UdpCodec` to create a simple UDP
//! client and server which speak a custom protocol.
//!
//! Here we're using the a custom codec to convert a UDP socket to a stream of
//! client messages. These messages are then processed and returned back as a
//! new message with a new destination. Overall, we then use this to construct a
//! "ping pong" pair where two sockets are sending messages back and forth.

extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use std::io;
use std::net::SocketAddr;
use std::str;

use futures::{Future, Stream, Sink};
use tokio_core::net::{UdpSocket, UdpCodec};
use tokio_core::reactor::Core;

pub struct LineCodec;

impl UdpCodec for LineCodec {
    type In = (SocketAddr, Vec<u8>);
    type Out = (SocketAddr, Vec<u8>);

    fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        Ok((*addr, buf.to_vec()))
    }

    fn encode(&mut self, (addr, buf): Self::Out, into: &mut Vec<u8>) -> SocketAddr {
        into.extend(buf);
        addr
    }
}

fn main() {
    drop(env_logger::init());

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Bind both our sockets and then figure out what ports we got.
    let a = UdpSocket::bind(&addr, &handle).unwrap();
    let b = UdpSocket::bind(&addr, &handle).unwrap();
    let b_addr = b.local_addr().unwrap();

    // We're parsing each socket with the `LineCodec` defined above, and then we
    // `split` each codec into the sink/stream halves.
    let (a_sink, a_stream) = a.framed(LineCodec).split();
    let (b_sink, b_stream) = b.framed(LineCodec).split();

    // Start off by sending a ping from a to b, afterwards we just print out
    // what they send us and continually send pings
    // let pings = stream::iter((0..5).map(Ok));
    let a = a_sink.send((b_addr, b"PING".to_vec())).and_then(|a_sink| {
        let mut i = 0;
        let a_stream = a_stream.take(4).map(move |(addr, msg)| {
            i += 1;
            println!("[a] recv: {}", String::from_utf8_lossy(&msg));
            (addr, format!("PING {}", i).into_bytes())
        });
        a_sink.send_all(a_stream)
    });

    // The second client we have will receive the pings from `a` and then send
    // back pongs.
    let b_stream = b_stream.map(|(addr, msg)| {
        println!("[b] recv: {}", String::from_utf8_lossy(&msg));
        (addr, b"PONG".to_vec())
    });
    let b = b_sink.send_all(b_stream);

    // Spawn the sender of pongs and then wait for our pinger to finish.
    handle.spawn(b.then(|_| Ok(())));
    drop(core.run(a));
}
