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
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use std::error::Error;
use std::net::SocketAddr;

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
                        // Transient errors (e.g. fd exhaustion) are recoverable,
                        // so we log and continue. A production server might add a
                        // backoff or break on fatal errors to avoid a busy loop.
                        eprintln!("failed to accept: {e}");
                        continue;
                    }
                };
                println!("accepted connection from {addr}");

                let token = token.clone();
                tracker.spawn(handle_connection(socket, addr, token));
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

async fn handle_connection(mut socket: TcpStream, addr: SocketAddr, token: CancellationToken) {
    tokio::select! {
        _ = echo(&mut socket) => {}
        _ = token.cancelled() => {
            notify_shutdown(&mut socket).await;
        }
    }

    println!("connection from {addr} closed");
}

/// Reads lines from the client and writes them back.
///
/// Called for every accepted connection. Runs until the client disconnects
/// or a read/write error occurs.
async fn echo(socket: &mut TcpStream) {
    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if writer.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
                line.clear();
            }
        }
    }
}

/// Sends a shutdown notice to the client before closing the connection.
///
/// Called when the cancellation token fires. Uses a timeout so that a
/// slow or unresponsive client cannot hold up the server shutdown.
async fn notify_shutdown(socket: &mut TcpStream) {
    let _ = time::timeout(
        Duration::from_secs(1),
        socket.write_all(b"server shutting down\n"),
    )
    .await;
}
