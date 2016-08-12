//! A small server that writes as many nul bytes on all connections it receives.
//!
//! There is no concurrency in this server, only one connection is written to at
//! a time.

#[macro_use]
extern crate futures;
extern crate futures_io;
extern crate futures_mio;

use std::env;
use std::io::{self, Write};
use std::net::SocketAddr;

use futures::Future;
use futures::stream::Stream;
use futures_io::IoFuture;

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let mut l = futures_mio::Loop::new().unwrap();
    let server = l.handle().tcp_listen(&addr).and_then(|socket| {
        socket.incoming().and_then(|(socket, addr)| {
            println!("got a socket: {}", addr);
            write(socket)
        }).for_each(|()| {
            println!("lost the socket");
            Ok(())
        })
    });
    println!("Listenering on: {}", addr);
    l.run(server).unwrap();
}

fn write(socket: futures_mio::TcpStream) -> IoFuture<()> {
    static BUF: &'static [u8] = &[0; 64 * 1024];
    socket.into_future().map_err(|e| e.0).and_then(move |(ready, mut socket)| {
        let ready = match ready {
            Some(ready) => ready,
            None => return futures::finished(()).boxed(),
        };
        while ready.is_write() {
            match socket.write(&BUF) {
                Ok(_) => {}
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return futures::failed(e).boxed(),
            }
        }
        write(socket)
    }).boxed()
}
