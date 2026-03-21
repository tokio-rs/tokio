//! Graceful shutdown example.
//!
//! This example follows the same approach described in the
//! [Graceful Shutdown tutorial](https://tokio.rs/tokio/topics/shutdown):
//!
//! - A [`CancellationToken`] tells tasks to stop accepting new work.
//! - A [`TaskTracker`] waits for in-flight work to complete.
//!
//! It runs a TCP echo server on `127.0.0.1:6142`. When Ctrl+C is
//! pressed, the server stops accepting connections and waits for all
//! active connections to finish before exiting.
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
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6142").await?;
    println!("listening on 127.0.0.1:6142");

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (socket, addr) = match result {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("failed to accept: {e}");
                        continue;
                    }
                };
                println!("accepted connection from {addr}");

                let token = token.clone();

                tracker.spawn(async move {
                    let (reader, mut writer) = socket.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut line = String::new();

                    loop {
                        tokio::select! {
                            result = reader.read_line(&mut line) => {
                                match result {
                                    Ok(0) | Err(_) => break,
                                    Ok(_) => {
                                        if writer.write_all(line.as_bytes()).await.is_err() {
                                            break;
                                        }
                                        line.clear();
                                    }
                                }
                            }
                            _ = token.cancelled() => {
                                let _ = writer.write_all(b"server shutting down\n").await;
                                break;
                            }
                        }
                    }

                    println!("connection from {addr} closed");
                });
            }

            _ = tokio::signal::ctrl_c() => {
                println!("\nshutdown signal received, waiting for connections to finish");
                break;
            }
        }
    }

    // Signal all tasks to stop and wait for them to complete.
    token.cancel();
    tracker.close();
    tracker.wait().await;

    println!("shutdown complete");
    Ok(())
}
