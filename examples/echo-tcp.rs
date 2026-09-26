//! A "hello world" echo server with Tokio
//!
//! This server will create a TCP listener, accept connections in a loop, and
//! write back everything that's read off of each TCP connection.
//!
//! Because the Tokio runtime uses a thread pool, each TCP connection is
//! processed concurrently with all other TCP connections across multiple
//! threads.
//!
//! To see this server in action, you can run this in one terminal:
//!
//!     cargo run --example echo-tcp
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect-tcp 127.0.0.1:8080
//!
//! Each line you type in to the `connect-tcp` terminal should be echo'd back to
//! you! If you open up multiple terminals running the `connect-tcp` example you
//! should be able to see them all make progress simultaneously.

#![warn(rust_2018_idioms)]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use std::env;
use std::error::Error;

const DEFAULT_ADDR: &str = "127.0.0.1:8080";
const BUFFER_SIZE: usize = 4096;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Allow passing an address to listen on as the first argument of this
    // program, but otherwise we'll just set up our TCP listener on
    // 127.0.0.1:8080 for connections.
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop.
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {addr}");

    loop {
        // Asynchronously wait for an inbound socket.
        let (mut socket, addr) = listener.accept().await?;

        // And this is where much of the magic of this server happens. We
        // crucially want all clients to make progress concurrently, rather than
        // blocking one on completion of another. To achieve this we use the
        // `tokio::spawn` function to execute the work in the background.
        //
        // Essentially here we're executing a new task to run concurrently,
        // which will allow all of our clients to be processed concurrently.

        tokio::spawn(async move {
            let mut buf = vec![0; BUFFER_SIZE];

            // In a loop, read data from the socket and write the data back.
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => {
                        // Connection closed by peer
                        return;
                    }
                    Ok(n) => {
                        // Write the data back. If writing fails, log the error and exit.
                        if let Err(e) = socket.write_all(&buf[0..n]).await {
                            eprintln!("Failed to write to socket {}: {}", addr, e);
                            return;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read from socket {}: {}", addr, e);
                        return;
                    }
                }
            }
        });
    }
}
