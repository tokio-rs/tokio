//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts connections, reads lines
//! from those connections, and broadcasts the lines to all other connected
//! clients. In a sense this is a bit of a "poor man's chat server".
//!
//! You can test this out by running:
//!
//!     cargo run --example chat
//!
//! And then in another window run:
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

extern crate futures;
extern crate futures_cpupool;
extern crate tokio;
extern crate tokio_io;

use std::collections::HashMap;
use std::iter;
use std::env;
use std::io::{Error, ErrorKind, BufReader};
use std::sync::{Arc, Mutex};

use futures::Future;
use futures::future::Executor;
use futures::stream::{self, Stream};
use futures_cpupool::CpuPool;
use tokio::net::TcpListener;
use tokio::reactor::Reactor;
use tokio_io::io;
use tokio_io::AsyncRead;

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse().unwrap();

    // Create the event loop and TCP listener we'll accept connections on.
    let mut core = Reactor::new().unwrap();
    let handle = core.handle();
    let socket = TcpListener::bind(&addr, &handle).unwrap();
    println!("Listening on: {}", addr);

    // This is currently a multi threaded server.
    //
    // Once the same thread executor lands, transition to single threaded.
    let connections = Arc::new(Mutex::new(HashMap::new()));

    let srv = socket.incoming().for_each(move |(stream, addr)| {
        println!("New Connection: {}", addr);
        let (reader, writer) = stream.split();

        // Create a channel for our stream, which other sockets will use to
        // send us messages. Then register our address with the stream to send
        // data to us.
        let (tx, rx) = futures::sync::mpsc::unbounded();
        connections.lock().unwrap().insert(addr, tx);

        // Define here what we do for the actual I/O. That is, read a bunch of
        // lines from the socket and dispatch them while we also write any lines
        // from other sockets.
        let connections_inner = connections.clone();
        let reader = BufReader::new(reader);

        // Model the read portion of this socket by mapping an infinite
        // iterator to each line off the socket. This "loop" is then
        // terminated with an error once we hit EOF on the socket.
        let iter = stream::iter_ok::<_, Error>(iter::repeat(()));
        let socket_reader = iter.fold(reader, move |reader, _| {
            // Read a line off the socket, failing if we're at EOF
            let line = io::read_until(reader, b'\n', Vec::new());
            let line = line.and_then(|(reader, vec)| {
                if vec.len() == 0 {
                    Err(Error::new(ErrorKind::BrokenPipe, "broken pipe"))
                } else {
                    Ok((reader, vec))
                }
            });

            // Convert the bytes we read into a string, and then send that
            // string to all other connected clients.
            let line = line.map(|(reader, vec)| {
                (reader, String::from_utf8(vec))
            });
            let connections = connections_inner.clone();
            line.map(move |(reader, message)| {
                println!("{}: {:?}", addr, message);
                let mut conns = connections.lock().unwrap();
                if let Ok(msg) = message {
                    // For each open connection except the sender, send the
                    // string via the channel.
                    let iter = conns.iter_mut()
                                    .filter(|&(&k, _)| k != addr)
                                    .map(|(_, v)| v);
                    for tx in iter {
                        tx.unbounded_send(format!("{}: {}", addr, msg)).unwrap();
                    }
                } else {
                    let tx = conns.get_mut(&addr).unwrap();
                    tx.unbounded_send("You didn't send valid UTF-8.".to_string()).unwrap();
                }
                reader
            })
        });

        // Whenever we receive a string on the Receiver, we write it to
        // `WriteHalf<TcpStream>`.
        let socket_writer = rx.fold(writer, |writer, msg| {
            let amt = io::write_all(writer, msg.into_bytes());
            let amt = amt.map(|(writer, _)| writer);
            amt.map_err(|_| ())
        });

        let pool = CpuPool::new(1);

        // Now that we've got futures representing each half of the socket, we
        // use the `select` combinator to wait for either half to be done to
        // tear down the other. Then we spawn off the result.
        let connections = connections.clone();
        let socket_reader = socket_reader.map_err(|_| ());
        let connection = socket_reader.map(|_| ()).select(socket_writer.map(|_| ()));
        pool.execute(connection.then(move |_| {
            connections.lock().unwrap().remove(&addr);
            println!("Connection {} closed.", addr);
            Ok(())
        })).unwrap();

        Ok(())
    });

    // execute server
    core.run(srv).unwrap();
}
