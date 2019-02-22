#![feature(await_macro, async_await, futures_api)]

#[macro_use]
extern crate tokio;

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::net::SocketAddr;

fn handle(mut stream: TcpStream) {
    tokio::spawn_async(async move {
        let mut buf = [0; 1024];

        loop {
            match await!(stream.read_async(&mut buf)).unwrap() {
                0 => break, // Socket closed
                n => {
                    // Send the data back
                    await!(stream.write_all_async(&buf[0..n])).unwrap();
                }
            }
        }
    });
}

fn main() {
    use std::env;

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Bind the TCP listener
    let listener = TcpListener::bind(&addr).unwrap();
    println!("Listening on: {}", addr);

    tokio::run_async(async {
        let mut incoming = listener.incoming();

        while let Some(stream) = await!(incoming.next()) {
            let stream = stream.unwrap();
            handle(stream);
        }
    });
}
