//! A chat server that broadcasts a message to all connections.

extern crate tokio_core;
extern crate futures;

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::iter;
use std::env;
use std::io::{Error, ErrorKind, BufReader};

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

    let future = socket.incoming().for_each(move |(stream, addr)| {
        let connections = connections.clone();
        let handle_inner = handle.clone();
        // We create a new future in which we create all other futures.
        // This makes `stream` be bound on the outer future's task, allowing
        // `ReadHalf` and `WriteHalf` to be shared between inner futures.
        let main_fn = move || {
            println!("New Connection: {}", addr);
            let (reader, writer) = stream.split();
            // channel to send messages to this connection from other futures
            let (tx, rx) = tokio_core::channel::channel(&handle_inner).unwrap();
            // add sender to hashmap of all current connections
            connections.borrow_mut().insert(addr, tx);

            let reader = BufReader::new(reader);
            let connections_inner = connections.clone();
            // https://users.rust-lang.org/t/loop-futures-for-client-handling/6950/2
            // First we need to get an infinite iterator
            let iter = stream::iter::<_, _, std::io::Error>(iter::repeat(()).map(Ok));
            // Then we fold it as infinite loop
            let socket_reader = iter.fold(reader, move |reader, _| {
                // read line
                let amt = io::read_until(reader, '\n' as u8, vec![]);
                // check if we hit EOF and need to close the connection
                let amt = amt.and_then(|(reader, vec)| {
                    // EOF was hit without reading a delimiter
                    if vec.len() == 0 {
                        let err = Error::new(ErrorKind::BrokenPipe, "Broken Pipe");
                        Err(err)
                    } else {
                        Ok((reader, vec))
                    }
                });
                // convert bytes into string
                let amt = amt.map(|(reader, vec)| (reader, String::from_utf8(vec)));
                let connections = connections_inner.clone();
                amt.map(move |(reader, message)| {
                    println!("{}: {:?}", addr, message);
                    let conns = connections.borrow_mut();
                    if let Ok(msg) = message {
                        // For each open connection except the sender, send the string
                        // via the channel
                        let iter = conns.iter().filter(|&(&k,_)| k != addr).map(|(_,v)| v);
                        for tx in iter {
                            tx.send(format!("{}: {}", addr, msg)).unwrap();
                        }
                    } else {
                        let tx = conns.get(&addr).unwrap();
                        tx.send("You didn't send valid UTF-8.".to_string()).unwrap();
                    }
                    reader
                })
            });

            // Whenever we receive a string on the Receiver, we write it to `WriteHalf<TcpStream>`.
            let socket_writer = rx.fold(writer, |writer, msg| {
                let amt = io::write_all(writer, msg.into_bytes());
                let amt = amt.map(|(writer, _)| writer);
                amt
            });

            // In order to fuse the reading and writing futures in the end, we need to have the
            // same output type. As we don't need the values anymore, we can just map them
            // to `()`.
            let socket_reader = socket_reader.map(|_| ());
            let socket_writer = socket_writer.map(|_| ());

            let amt = socket_reader.select(socket_writer);
            amt.then(move |_| {
                connections.borrow_mut().remove(&addr);
                println!("Connection {} closed.", addr);
                Ok(())
            })
        };
        handle.spawn_fn(main_fn);
        Ok(())
    });

    // exectue server
    core.run(future).unwrap();
}

