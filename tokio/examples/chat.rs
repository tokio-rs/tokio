//! A chat server that broadcasts a message to all connections.
//!
//! This example is explicitly more verbose than it has to be. This is to
//! illustrate more concepts.
//!
//! A chat server for telnet clients. After a telnet client connects, the first
//! line should contain the client's name. After that, all lines sent by a
//! client are broadcasted to all other connected clients.
//!
//! Because the client is telnet, lines are delimited by "\r\n".
//!
//! You can test this out by running:
//!
//!     cargo run --example chat
//!
//! And then in another terminal run:
//!
//!     telnet localhost 6142
//!
//! You can run the `telnet` command in any number of additional windows.
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

#![feature(async_await)]
#![deny(warnings, rust_2018_idioms)]

use futures::channel::mpsc;
use futures::lock::Mutex;
use futures::task::Context;
use futures::Stream;
use futures::{SinkExt, StreamExt};

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use tokio;
use tokio::codec::{Framed, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<String>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<String>;

/// Data that is shared between all peers in the chat server.
///
/// This is the set of `Tx` handles for all connected clients. Whenever a
/// message is received from a client, it is broadcasted to all peers by
/// iterating over the `peers` entries and sending a copy of the message on each
/// `Tx`.
struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}

/// The state for each connected client.
struct Peer {
    /// The TCP socket wrapped with the `Lines` codec, defined below.
    ///
    /// This handles sending and receiving data on the socket. When using
    /// `Lines`, we can work at the line level instead of having to manage the
    /// raw byte operations.
    lines: Framed<TcpStream, LinesCodec>,

    /// Receive half of the message channel.
    ///
    /// This is used to receive messages from peers. When a message is received
    /// off of this `Rx`, it will be written to the socket.
    rx: Rx,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    async fn broadcast(
        &mut self,
        sender: SocketAddr,
        message: &str,
    ) -> Result<(), futures::channel::mpsc::SendError> {
        for peer in self.peers.iter_mut() {
            if *peer.0 != sender {
                peer.1.send(message.into()).await?;
            }
        }

        Ok(())
    }
}

impl Peer {
    /// Create a new instance of `Peer`.
    async fn new(state: Arc<Mutex<Shared>>, lines: Framed<TcpStream, LinesCodec>) -> Peer {
        // Get the client socket address
        let addr = lines.get_ref().peer_addr().unwrap();

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded();

        // Add an entry for this `Peer` in the shared state map.
        state.lock().await.peers.insert(addr, tx);

        Peer { lines, rx }
    }
}

#[derive(Debug)]
enum Message {
    MessageToSend(String),
    MessageToReceieve(String),
}

impl Stream for Peer {
    type Item = Result<Message, ()>; // #TODO LinesCodecError

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while let Poll::Ready(Some(v)) = self.rx.poll_next_unpin(cx) {
            return Poll::Ready(Some(Ok(Message::MessageToReceieve(v))));
        }

        match self.lines.poll_next_unpin(cx) {
            Poll::Ready(Some(v)) => {
                // #TODO LinesCodecError
                /*let res = match v {
                    Ok(msg) => Ok(Message::MessageToSend(msg)),
                    Err(e) => Err(e),
                };
                return Poll::Ready(Some(res));*/

                return Poll::Ready(Some(Ok(Message::MessageToSend(v.unwrap()))));
            }
            Poll::Ready(None) => {
                return Poll::Ready(None);
            }
            _ => {}
        }

        Poll::Pending
    }
}

async fn process(
    state: Arc<Mutex<Shared>>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut lines = Framed::new(stream, LinesCodec::new());

    let username = lines.next().await.unwrap()?;

    let mut peer = Peer::new(state.clone(), lines).await;

    {
        let mut state = state.lock().await;
        let msg = format!("{} has joined the chat", username);
        state.broadcast(addr, &msg).await?;
    }

    while let Ok(msg) = peer.next().await.unwrap() {
        match msg {
            Message::MessageToSend(msg) => {
                let mut state = state.lock().await;
                let msg = format!("{}: {}", username, msg);

                state.broadcast(addr, &msg).await?;
            }
            Message::MessageToReceieve(msg) => {
                peer.lines.send(msg).await?;
            }
        }
    }

    {
        let mut state = state.lock().await;
        state.peers.remove(&addr);

        let msg = format!("{} has left the chat", username);
        state.broadcast(addr, &msg).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create the shared state. This is how all the peers communicate.
    //
    // The server task will hold a handle to this. For every new client, the
    // `state` handle is cloned and passed into the task that processes the
    // client connection.
    let state = Arc::new(Mutex::new(Shared::new()));

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:6142".to_string());
    let addr = addr.parse::<SocketAddr>()?;

    // Bind a TCP listener to the socket address.
    //
    // Note that this is the Tokio TcpListener, which is fully async.
    let mut listener = TcpListener::bind(&addr)?;

    println!("server running on {}", addr);

    loop {
        // Asynchronously wait for an inbound TcpStream.
        let (stream, addr) = listener.accept().await?;

        let state = state.clone();

        tokio::spawn(async move {
            if let Err(e) = process(state, stream, addr).await {
                println!("an error occured; error = {:?}", e);
            }
        });
    }
}
