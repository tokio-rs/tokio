//! Hello world server.
//!
//! A simple server that accepts connections, writes "hello world\n", and closes
//! the connection.
//!
//! You can test this out by running:
//!
//!     cargo run --example hello_world
//!
//! And then in another terminal run:
//!
//!     telnet localhost 6142
//!

#![deny(warnings)]

extern crate tokio;
extern crate tokio_io;
extern crate futures;

use tokio::executor::current_thread;
use tokio::net::TcpListener;
use tokio_io::io;
use futures::{Future, Stream};

pub fn main() {
    let addr = "127.0.0.1:6142".parse().unwrap();

    // Bind a TCP listener to the socket address.
    //
    // Note that this is the Tokio TcpListener, which is fully async.
    let listener = TcpListener::bind(&addr).unwrap();

    // The server task asynchronously iterates over and processes each
    // incoming connection.
    let server = listener.incoming().for_each(|socket| {
        println!("accepted socket; addr={:?}", socket.peer_addr().unwrap());

        let connection = io::write_all(socket, "hello world\n")
            .then(|res| {
                println!("wrote message; success={:?}", res.is_ok());
                Ok(())
            });

        // Spawn a new task that processes the socket:
        current_thread::spawn(connection);

        Ok(())
    })
    .map_err(|err| {
        // All tasks must have an `Error` type of `()`. This forces error
        // handling and helps avoid silencing failures.
        //
        // In our example, we are only going to log the error to STDOUT.
        println!("accept error = {:?}", err);
    });

    println!("server running on localhost:6142");

    // This starts the `current_thread` executor.
    //
    // Executors are responsible for scheduling many asynchronous tasks, driving
    // them to completion. There are a number of different executor
    // implementations, each providing different scheduling characteristics.
    //
    // The `current_thread` executor multiplexes all scheduled tasks on the
    // current thread. This means that spawned tasks must not implement `Send`.
    // It's important to note that all futures / tasks are lazy. No work will
    // happen unless they are spawned onto an executor.
    //
    // The executor will start running the `server` task, which, in turn, spawns
    // new tasks for each incoming connection.
    //
    // The `current_thread::block_on_all` function will block until *all*
    // spawned tasks complete.
    //
    // In our example, we have not defined a shutdown strategy, so this will
    // block until `ctrl-c` is pressed at the terminal.
    current_thread::block_on_all(server).unwrap();
}
