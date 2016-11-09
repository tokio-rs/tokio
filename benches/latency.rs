#![feature(test)]

extern crate test;
extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use test::Bencher;
use std::thread;
use std::time::Duration;

use futures::stream::Stream;

/// UDP echo server
struct EchoServer {
    socket : UdpSocket,
    buf : Vec<u8>,
    to_send : Option<(usize, SocketAddr)>,
    stop : Arc<AtomicBool>,
}

impl EchoServer {
    fn new(s : UdpSocket, stop : Arc<AtomicBool>) -> Self {
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
            if self.stop.load(Ordering::SeqCst) {
                return Ok(futures::Async::Ready(()))
            }

            if let Some((size, peer)) = self.to_send.take() {
                match self.socket.send_to(&self.buf[..size], &peer) {
                    Err(e) => {
                        self.to_send = Some((size, peer));
                        return try_nb!(Err(e));
                    },
                    Ok(_) => { }
                }
            }

            self.to_send = Some(
                try_nb!(self.socket.recv_from(&mut self.buf))
                );
        }
    }
}

/// UDP echo server
struct ChanServer {
    rx : tokio_core::channel::Receiver<usize>,
    tx : tokio_core::channel::Sender<usize>,
    buf : Option<usize>,
    stop : Arc<AtomicBool>,
}

impl ChanServer {
    fn new(tx : tokio_core::channel::Sender<usize>, rx : tokio_core::channel::Receiver<usize>, stop : Arc<AtomicBool>) -> Self {
        ChanServer {
            rx: rx,
            tx: tx,
            buf: None,
            stop: stop,
        }
    }
}

impl Future for ChanServer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            if self.stop.load(Ordering::SeqCst) {
                return Ok(futures::Async::Ready(()))
            }

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
    let server_addr= "127.0.0.1:7398".to_string();
    let server_addr = server_addr.parse::<SocketAddr>().unwrap();
    let client_addr= "127.0.0.1:7399".to_string();
    let client_addr = client_addr.parse::<SocketAddr>().unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();

    let child = thread::spawn(move || {
        let mut l = Core::new().unwrap();
        let handle = l.handle();

        let socket = tokio_core::net::UdpSocket::bind(&server_addr, &handle).unwrap();

        let server = EchoServer::new(socket, stop);

        l.run(server).unwrap();
    });

    // TODO: More reliable way to bind server socket and start server
    // first
    thread::sleep(Duration::from_millis(100));

    let client = std::net::UdpSocket::bind(client_addr).unwrap();

    let mut buf = [0u8; 1000];

    // warmup phase; for some reason initial couple of
    // rounds is much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..1000 {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    }

    b.iter(|| {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    });

    // Stop the server; TODO: Use better method
    stop2.store(true, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(1));
    client.send_to(&buf, &server_addr).unwrap();

    child.join().unwrap();
}

#[bench]
fn channel_latency(b: &mut Bencher) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();

    // TODO: Any way to start Loop on a separate thread and yet get
    // a tokio_core::channel to it?
    let (tx, rx) = std::sync::mpsc::channel();

    let child = thread::spawn(move || {
        let mut l = Core::new().unwrap();
        let handle = l.handle();

        let (in_tx, in_rx) = tokio_core::channel::channel(&handle).unwrap();
        let (out_tx, out_rx) = tokio_core::channel::channel(&handle).unwrap();

        let server = ChanServer::new(out_tx, in_rx, stop);

        tx.send((in_tx, out_rx)).unwrap();
        l.run(server).unwrap();
    });

    let (in_tx, out_rx) = rx.recv().unwrap();

    // TODO: More reliable way to bind server socket and start server
    // first
    thread::sleep(Duration::from_millis(100));

    let mut rx_iter = out_rx.wait();

    // warmup phase; for some reason initial couple of
    // rounds is much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..1000 {
        in_tx.send(1usize).unwrap();
        let _ = rx_iter.next();
    }

    b.iter(|| {
        in_tx.send(1usize).unwrap();
        let _ = rx_iter.next();
    });

    // Stop the server; TODO: Use better method
    stop2.store(true, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(1));
    in_tx.send(1usize).unwrap();

    child.join().unwrap();
}
