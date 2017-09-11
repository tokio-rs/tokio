//! A small example of a server that accepts TCP connections and writes out
//! `Hello!` to them, afterwards closing the connection.
//!
//! You can test this out by running:
//!
//!     cargo run --example hello
//!
//! and then in another terminal executing
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! You should see `Hello!` printed out and then the `nc` program will exit.

extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use std::env;
use std::net::SocketAddr;

use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;

fn main() {
    env_logger::init().unwrap();
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let mut core = Core::new().unwrap();
    let listener = TcpListener::bind(&addr, &core.handle()).unwrap();

    let addr = listener.local_addr().unwrap();
    println!("Listening for connections on {}", addr);

    let clients = listener.incoming();
    let welcomes = clients.and_then(|(socket, _peer_addr)| {
        tokio_io::io::write_all(socket, b"Hello!\n")
    });
    let server = welcomes.for_each(|(_socket, _welcome)| {
        Ok(())
    });

    core.run(server).unwrap();
}
