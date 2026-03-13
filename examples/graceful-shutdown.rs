//! Graceful shutdown example using `tokio::signal`.
//!
//! This starts a TCP echo server that shuts down cleanly when it receives
//! Ctrl+C (SIGINT). In-flight connections are allowed to finish before
//! the process exits.
//!
//! Start the server:
//!
//!     cargo run --example graceful-shutdown
//!
//! Then connect with:
//!
//!     nc 127.0.0.1 6142
//!
//! Press Ctrl+C on the server to trigger a graceful shutdown.

#![warn(rust_2018_idioms)]

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6142").await?;
    println!("listening on 127.0.0.1:6142");

    // A broadcast channel to notify all tasks when shutdown is requested.
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    loop {
        tokio::select! {
            // Wait for a new connection.
            result = listener.accept() => {
                let (socket, addr) = result?;
                println!("accepted connection from {addr}");

                let mut shutdown_rx = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    let (reader, mut writer) = socket.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut line = String::new();

                    loop {
                        tokio::select! {
                            // Echo lines back to the client.
                            result = reader.read_line(&mut line) => {
                                match result {
                                    Ok(0) | Err(_) => break,
                                    Ok(_) => {
                                        let _ = writer.write_all(line.as_bytes()).await;
                                        line.clear();
                                    }
                                }
                            }
                            // Stop this connection when shutdown is signaled.
                            _ = shutdown_rx.recv() => {
                                let _ = writer.write_all(b"server shutting down\n").await;
                                break;
                            }
                        }
                    }

                    println!("connection from {addr} closed");
                });
            }

            // Wait for Ctrl+C.
            _ = tokio::signal::ctrl_c() => {
                println!("\nshutdown signal received, closing listener");
                break;
            }
        }
    }

    // Notify all active connections.
    let _ = shutdown_tx.send(());

    // Drop the sender and wait briefly for connections to finish.
    drop(shutdown_tx);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    println!("shutdown complete");
    Ok(())
}
