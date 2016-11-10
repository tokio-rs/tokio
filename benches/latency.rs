#![feature(test)]

extern crate test;
extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use test::Bencher;
use std::thread;

use futures::stream::Stream;
use futures::{oneshot, Oneshot};

/// UDP echo server
struct EchoServer {
    socket : UdpSocket,
    buf : Vec<u8>,
    to_send : Option<(usize, SocketAddr)>,
    stop : Oneshot<()>,
}

impl EchoServer {
    fn new(s : UdpSocket, stop : Oneshot<()>) -> Self {
        EchoServer {
            socket: s,
            to_send: None,
            buf: vec![0u8; 1600],
            stop: stop,
        }
    }
}

impl Future for EchoServer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            if self.stop.poll() != Ok(futures::Async::NotReady) {
                return Ok(futures::Async::Ready(()))
            }

            if let Some(&(size, peer)) = self.to_send.as_ref() {
                let _ = try_nb!(self.socket.send_to(&self.buf[..size], &peer));
            }
            self.to_send = None;

            self.to_send = Some(
                try_nb!(self.socket.recv_from(&mut self.buf))
                );
        }
    }
}

/// UDP echo server
///
/// TODO: This may be replace-able with the newly-minted Sink::send_all function in the futures crate
struct ChanServer {
    rx : tokio_core::channel::Receiver<usize>,
    tx : tokio_core::channel::Sender<usize>,
    buf : Option<usize>,
}

impl ChanServer {
    fn new(tx : tokio_core::channel::Sender<usize>, rx : tokio_core::channel::Receiver<usize>) -> Self {
        ChanServer {
            rx: rx,
            tx: tx,
            buf: None,
        }
    }
}

impl Future for ChanServer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            if let Some(u) = self.buf.take() {
                match self.tx.send(u) {
                    Err(e) => {
                        self.buf = Some(u);
                        return try_nb!(Err(e));
                    },
                    Ok(_) => { }
                }
            }

            match self.rx.poll() {
                Ok(futures::Async::Ready(None)) => return Ok(futures::Async::Ready(())),
                Ok(futures::Async::Ready(Some(t))) => { self.buf = Some(t) },
                Ok(futures::Async::NotReady) => return Ok(futures::Async::NotReady),
                Err(e) => return try_nb!(Err(e)),
            }
        }
    }
}

#[bench]
fn udp_echo_latency(b: &mut Bencher) {
    let server_addr= "127.0.0.1:0".to_string();
    let server_addr = server_addr.parse::<SocketAddr>().unwrap();
    let client_addr= "127.0.0.1:0".to_string();
    let client_addr = client_addr.parse::<SocketAddr>().unwrap();

    let (stop_c, stop_p) = oneshot::<()>();

    let (tx, rx) = std::sync::mpsc::channel();

    let child = thread::spawn(move || {
        let mut l = Core::new().unwrap();
        let handle = l.handle();

        let socket = tokio_core::net::UdpSocket::bind(&server_addr, &handle).unwrap();

        tx.send(socket.local_addr().unwrap()).unwrap();
        let server = EchoServer::new(socket, stop_p);

        l.run(server).unwrap();
    });


    let client = std::net::UdpSocket::bind(client_addr).unwrap();

    let server_addr = rx.recv().unwrap();
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
fn channel_latency(b: &mut Bencher) {

    let (tx, rx) = std::sync::mpsc::channel();

    let child = thread::spawn(move || {
        let mut l = Core::new().unwrap();
        let handle = l.handle();

        let (in_tx, in_rx) = tokio_core::channel::channel(&handle).unwrap();
        let (out_tx, out_rx) = tokio_core::channel::channel(&handle).unwrap();

        let server = ChanServer::new(out_tx, in_rx);

        tx.send((in_tx, out_rx)).unwrap();
        l.run(server).unwrap();
    });

    let (in_tx, out_rx) = rx.recv().unwrap();

    let mut rx_iter = out_rx.wait();

    // warmup phase; for some reason initial couple of runs are much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..8 {
        in_tx.send(1usize).unwrap();
        let _ = rx_iter.next();
    }

    b.iter(|| {
        in_tx.send(1usize).unwrap();
        let _ = rx_iter.next();
    });

    drop(in_tx);

    child.join().unwrap();
}
