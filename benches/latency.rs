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

/// UDP echo server
struct Server {
    socket : UdpSocket,
    buf : Vec<u8>,
    to_send : Option<(usize, SocketAddr)>,
    stop : Arc<AtomicBool>,
}

impl Server {
    fn new(s : UdpSocket, stop : Arc<AtomicBool>) -> Self {
        Server {
            socket: s,
            to_send: None,
            buf: vec![0u8; 1600],
            stop: stop,
        }
    }
}

impl Future for Server {
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

        let server = Server::new(socket, stop);

        l.run(server).unwrap();
    });

    // TODO: More reliable way to bind server socket and start server
    // first
    thread::sleep(Duration::from_millis(100));

    let client = std::net::UdpSocket::bind(client_addr).unwrap();

    let mut buf = [0u8; 1000];
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
