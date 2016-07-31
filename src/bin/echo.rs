//! An echo server that just writes back everything that's written to it.

extern crate futures;
extern crate futures_io;
extern crate futures_mio;

use std::env;
use std::net::SocketAddr;

use futures::Future;
use futures_io::{copy, TaskIo};
use futures::stream::Stream;

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Create the event loop that will drive this server
    let mut l = futures_mio::Loop::new().unwrap();

    // Create a TCP listener which will listen for incoming connections
    let server = l.handle().tcp_listen(&addr);

    let done = server.and_then(move |socket| {
        // Once we've got the TCP listener, inform that we have it
        println!("Listenering on: {}", addr);

        // Pull out the stream of incoming connections and then for each new
        // one spin up a new task copying data. We put the `socket` into a
        // `TaskIo` structure which then allows us to `split` it into the read
        // and write halves of the socket.
        //
        // Finally we use the `io::copy` future to copy all data from the
        // reading half onto the writing half.
        socket.incoming().for_each(|(socket, addr)| {
            let io = TaskIo::new(socket);
            let pair = io.map(|io| io.split());
            let amt = pair.and_then(|(reader, writer)| {
                copy(reader, writer)
            });

            // Once all that is done we print out how much we wrote, and then
            // critically we *forget* this future which allows it to run
            // concurrently with other connections.
            amt.map(move |amt| {
                println!("wrote {} bytes to {}", amt, addr)
            }).forget();

            Ok(())
        })
    });
    l.run(done).unwrap();
}
