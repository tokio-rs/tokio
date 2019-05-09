#![feature(await_macro, async_await)]

use tokio::async_wait;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::net::SocketAddr;

fn handle(mut stream: TcpStream) {
    tokio::spawn_async(async move {
        let mut buf = [0; 1024];

        loop {
            match async_wait!(stream.read_async(&mut buf)).unwrap() {
                0 => break, // Socket closed
                n => {
                    // Send the data back
                    async_wait!(stream.write_all_async(&buf[0..n])).unwrap();
                }
            }
        }
    });
}

#[tokio::main]
async fn main() {
    use std::env;

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Bind the TCP listener
    let listener = TcpListener::bind(&addr).unwrap();
    println!("Listening on: {}", addr);

    let mut incoming = listener.incoming();

    while let Some(stream) = async_wait!(incoming.next()) {
        let stream = stream.unwrap();
        handle(stream);
    }
}
