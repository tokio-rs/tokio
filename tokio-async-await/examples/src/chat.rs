#![feature(await_macro, async_await, futures_api)]

#[macro_use]
extern crate tokio;

use tokio::codec::{LinesCodec, Decoder};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::sync::mpsc;

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<String>;

struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }
}

async fn process(stream: TcpStream, state: Arc<Mutex<Shared>>) -> io::Result<()> {
    let addr = stream.peer_addr().unwrap();
    let mut lines = LinesCodec::new().framed(stream);

    // Extract the peer's name
    let name = match await!(lines.next()) {
        Some(name) => name?,
        None => {
            // Disconnected early
            return Ok(());
        }
    };

    println!("`{}` is joining the chat", name);

    let (tx, mut rx) = mpsc::unbounded();

    // Register the socket
    state.lock().unwrap()
        .peers.insert(addr, tx);

    // Split the `lines` handle into send and recv handles. This allows spawning
    // separate tasks.
    let (mut lines_tx, mut lines_rx) = lines.split();

    // Spawn a task that receives all lines broadcasted to us from other peers
    // and writes it to the client.
    tokio::spawn_async(async move {
        while let Some(line) = await!(rx.next()) {
            let line = line.unwrap();
            await!(lines_tx.send_async(line));
        }
    });

    // Use the current task to read lines from the socket and broadcast them to
    // other peers.
    while let Some(message) = await!(lines_rx.next()) {
        // TODO: Error handling
        let message = message.unwrap();

        let mut line = name.clone();
        line.push_str(": ");
        line.push_str(&message);
        line.push_str("\r\n");

        let state = state.lock().unwrap();

        for (peer_addr, tx) in &state.peers {
            if *peer_addr != addr {
                // TODO: Error handling
                tx.unbounded_send(line.clone()).unwrap();
            }
        }
    }

    // Remove the client from the shared state. Doing so will also result in the
    // tx task to terminate.
    state.lock().unwrap()
        .peers.remove(&addr)
        .expect("bug");

    Ok(())
}

fn main() {
    // Create the shared state. This is how all the peers communicate.
    //
    // The server task will hold a handle to this. For every new client, the
    // `state` handle is cloned and passed into the task that processes the
    // client connection.
    let state = Arc::new(Mutex::new(Shared::new()));

    let addr = "127.0.0.1:6142".parse().unwrap();

    // Bind a TCP listener to the socket address.
    //
    // Note that this is the Tokio TcpListener, which is fully async.
    let listener = TcpListener::bind(&addr).unwrap();

    println!("server running on localhost:6142");

    // Start the Tokio runtime.
    tokio::run_async(async move {
        let mut incoming = listener.incoming();

        while let Some(stream) = await!(incoming.next()) {
            let stream = match stream {
                Ok(stream) => stream,
                Err(_) => continue,
            };

            let state = state.clone();

            tokio::spawn_async(async move {
                if let Err(_) = await!(process(stream, state)) {
                    eprintln!("failed to process connection");
                }
            });
        }
    });
}

