//! An UDP echo server that just sends back everything that it receives.

extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::{env, io};
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

/// UDP echo server
struct Server {
    socket : UdpSocket,
    buf : Vec<u8>,
    to_send : Option<(usize, SocketAddr)>,
}

impl Server {
    fn new(s : UdpSocket) -> Self {
        Server {
            socket: s,
            to_send: None,
            buf: vec![0u8; 1600],
        }
    }
}

impl Future for Server {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            if let Some((size, peer)) = self.to_send.take() {
                match self.socket.send_to(&self.buf[..size], &peer) {
                    Err(e) => {
                        self.to_send = Some((size, peer));
                        return try_nb!(Err(e));
                    },
                    Ok(_) => {
                        println!("Echoed {} bytes", size);
                    }
                }
            }

            self.to_send = Some(
                try_nb!(self.socket.recv_from(&mut self.buf))
                );
        }
    }
}

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Create the event loop that will drive this server
    let mut l = Core::new().unwrap();
    let handle = l.handle();

    // Create and bind an UDP socket
    let socket = UdpSocket::bind(&addr, &handle).unwrap();

    // Inform that we have are listening
    println!("Listening on: {}", addr);

    // Create a server future
    let server = Server::new(socket);

    // Start event loop with the initial future: UDP echo server
    l.run(server).unwrap();
}
