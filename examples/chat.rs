//! A chat server that broadcasts a message to all connections.

extern crate tokio_core;
extern crate futures;

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::iter;
use std::env;
use std::io::BufReader;

use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_core::io::{self, Io};

use futures::stream::{self, Stream};
use futures::Future;

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse().unwrap();
    // We are single-threaded, so we can just use Rc and RefCell.
    let connections = Rc::new(RefCell::new(HashMap::new()));

    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let socket = TcpListener::bind(&addr, &handle).unwrap();
    println!("Listening on: {}", addr);

    let connections = connections.clone();
    let future = socket.incoming().for_each(move |(stream, addr)| {
        let connections = connections.clone();
        let handle_inner = handle.clone();
        // We create a new future in which we create all other futures.
        // This makes `stream` be bound on the `lazy` future's task, allowing
        // `ReadHalf` and `WriteHalf` to be shared between inner futures.
        handle.spawn_fn(move || {
            println!("New Connection: {}", addr);
            let (reader, writer) = stream.split();
            // channel to send messages to this connection from other futures
            let (tx, rx) = tokio_core::channel::channel(&handle_inner).unwrap();
            // add sender to hashmap of all current connections
            connections.borrow_mut().insert(addr, tx);

            let connections_inner = connections.clone();
            // https://users.rust-lang.org/t/loop-futures-for-client-handling/6950/2
            // We have an endless loop reading from a client.
            // In order to fuse the reading and writing futures in the end, we need to have the same
            // output type. Therefore we use `(Option<BufReader<ReadHalf<TcpStream>>>,
            // Option<WriteHalf<TcpStream>>)`.
            let reader = BufReader::new(reader);
            let socket_reader = stream::iter::<_, _, std::io::Error>(iter::repeat(()).map(Ok)).fold((Some(reader),None), move |(reader, _), _| {
                let reader = reader.unwrap();
                let connections = connections_inner.clone();
                // read and parse length prefix
                io::read_until(reader, '\n' as u8, vec![])
                    .and_then(|(reader, vec)| futures::lazy(|| {
                        // EOF was hit without reading a delimiter
                        if vec.len() == 0 {
                            futures::failed((std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Broken Pipe"))).boxed()
                        } else {
                            futures::finished((reader, vec)).boxed()
                        }
                    }))
                    // convert bytes into string
                    .map(|(reader, vec)| (reader, String::from_utf8(vec).unwrap()))
                    .and_then(move |(reader, message)| {
                        println!("{}: {:?}", addr, message);
                        // For each open connection except the sender, send the string via the channel
                        for tx in connections.borrow_mut().iter().filter(|&(&k,_)| k != addr).map(|(_,v)| v) {
                            tx.send(message.clone()).unwrap();
                        }
                        futures::finished((Some(reader),None))
                    })
            });

            // Whenever we receive a string on the Receiver, we write it to `WriteHalf<TcpStream>`.
            let socket_writer = rx.fold((None, Some(writer)), move |(_, writer), msg| {
                let writer = writer.unwrap();
                io::write_all(writer, msg.into_bytes()).map(|(writer, _)| (None, Some(writer))).boxed()
            });

            socket_reader.select(socket_writer)
                .then(move |_| {
                    connections.borrow_mut().remove(&addr);
                    println!("Connection {:?} closed.", addr);
                    Ok(())
                })
        });
        Ok(())
    });

    // exectue server
    core.run(future).unwrap();
}
