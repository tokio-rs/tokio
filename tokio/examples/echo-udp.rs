//! An UDP echo server that just sends back everything that it receives.
//!
//! If you're on Unix you can test this out by in one terminal executing:
//!
//!     cargo run --example echo-udp
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect -- --udp 127.0.0.1:8080
//!
//! Each line you type in to the `nc` terminal should be echo'd back to you!

#![deny(warnings)]

#[macro_use]
extern crate futures;
extern crate tokio;

use std::net::SocketAddr;
use std::{env, io};

use tokio::net::UdpSocket;
use tokio::prelude::*;

struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl Future for Server {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = self.to_send {
                let amt = try_ready!(self.socket.poll_send_to(&self.buf[..size], &peer));
                println!("Echoed {}/{} bytes to {}", amt, size, peer);
                self.to_send = None;
            }

            // If we're here then `to_send` is `None`, so we take a look for the
            // next message we're going to echo back.
            self.to_send = Some(try_ready!(self.socket.poll_recv_from(&mut self.buf)));
        }
    }
}

fn main() -> Result<(), Box<std::error::Error>> {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>()?;

    let socket = UdpSocket::bind(&addr)?;
    println!("Listening on: {}", socket.local_addr()?);

    let server = Server {
        socket: socket,
        buf: vec![0; 1024],
        to_send: None,
    };

    // This starts the server task.
    //
    // `map_err` handles the error by logging it and maps the future to a type
    // that can be spawned.
    //
    // `tokio::run` spawns the task on the Tokio runtime and starts running.
    tokio::run(server.map_err(|e| println!("server error = {:?}", e)));
    Ok(())
}
