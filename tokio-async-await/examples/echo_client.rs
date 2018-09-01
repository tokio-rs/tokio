#![feature(await_macro, async_await, futures_api)]

#[macro_use]
extern crate tokio;

use tokio::net::TcpStream;
use tokio::prelude::*;

use std::io;
use std::net::SocketAddr;

const MESSAGES: &[&str] = &[
    "hello",
    "world",
    "one two three",
];

async fn run_client(addr: &SocketAddr) -> io::Result<()> {
    let mut stream = await!(TcpStream::connect(addr))?;

    // Buffer to read into
    let mut buf = [0; 128];

    for msg in MESSAGES {
        println!(" > write = {:?}", msg);

        // Write the message to the server
        await!(stream.write_all_async(msg.as_bytes()))?;

        // Read the message back from the server
        await!(stream.read_exact_async(&mut buf[..msg.len()]))?;

        assert_eq!(&buf[..msg.len()], msg.as_bytes());
    }

    Ok(())
}

fn main() {
    use std::env;

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Connect to the echo serveer

    tokio::run_async(async move {
        match await!(run_client(&addr)) {
            Ok(_) => println!("done."),
            Err(e) => eprintln!("echo client failed; error = {:?}", e),
        }
    });
}
