#![feature(test)]

extern crate test;
extern crate futures;
#[macro_use]
extern crate tokio;
#[macro_use]
extern crate tokio_io;

use std::io;
use std::net::SocketAddr;
use std::thread;

use futures::sync::oneshot;
use futures::sync::mpsc;
use futures::{Future, Poll, Sink, Stream};
use test::Bencher;
use tokio::net::UdpSocket;
use tokio::reactor::Reactor;

/// UDP echo server
struct EchoServer {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl EchoServer {
    fn new(s: UdpSocket) -> Self {
        EchoServer {
            socket: s,
            to_send: None,
            buf: vec![0u8; 1600],
        }
    }
}

impl Future for EchoServer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            if let Some(&(size, peer)) = self.to_send.as_ref() {
                try_nb!(self.socket.send_to(&self.buf[..size], &peer));
                self.to_send = None;
            }
            self.to_send = Some(try_nb!(self.socket.recv_from(&mut self.buf)));
        }
    }
}

#[bench]
fn udp_echo_latency(b: &mut Bencher) {
    let any_addr = "127.0.0.1:0".to_string();
    let any_addr = any_addr.parse::<SocketAddr>().unwrap();

    let (stop_c, stop_p) = oneshot::channel::<()>();
    let (tx, rx) = oneshot::channel();

    let child = thread::spawn(move || {
        let mut l = Reactor::new().unwrap();
        let handle = l.handle();

        let socket = tokio::net::UdpSocket::bind(&any_addr, &handle).unwrap();
        tx.complete(socket.local_addr().unwrap());

        let server = EchoServer::new(socket);
        let server = server.select(stop_p.map_err(|_| panic!()));
        let server = server.map_err(|_| ());
        l.run(server).unwrap()
    });


    let client = std::net::UdpSocket::bind(&any_addr).unwrap();

    let server_addr = rx.wait().unwrap();
    let mut buf = [0u8; 1000];

    // warmup phase; for some reason initial couple of
    // runs are much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..8 {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    }

    b.iter(|| {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    });

    stop_c.complete(());
    child.join().unwrap();
}

#[bench]
fn futures_channel_latency(b: &mut Bencher) {
    let (mut in_tx, in_rx) = mpsc::channel(32);
    let (out_tx, out_rx) = mpsc::channel::<_>(32);

    let child = thread::spawn(|| out_tx.send_all(in_rx.then(|r| r.unwrap())).wait());
    let mut rx_iter = out_rx.wait();

    // warmup phase; for some reason initial couple of runs are much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..8 {
        in_tx.start_send(Ok(1usize)).unwrap();
        let _ = rx_iter.next();
    }

    b.iter(|| {
        in_tx.start_send(Ok(1usize)).unwrap();
        let _ = rx_iter.next();
    });

    drop(in_tx);
    child.join().unwrap().unwrap();
}
