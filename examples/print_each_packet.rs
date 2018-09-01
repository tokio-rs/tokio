//! A "print-each-packet" server with Tokio
//!
//! This server will create a TCP listener, accept connections in a loop, and
//! put down in the stdout everything that's read off of each TCP connection.
//!
//! Because the Tokio runtime uses a thread pool, each TCP connection is
//! processed concurrently with all other TCP connections across multiple
//! threads.
//!
//! To see this server in action, you can run this in one terminal:
//!
//!     cargo run --example print\_each\_packet
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! Each line you type in to the `connect` terminal should be written to terminal!
//!
//! Minimal js example:
//!
//! ```js
//! var net = require("net");
//!
//! var listenPort = 8080;
//!
//! var server = net.createServer(function (socket) {
//!     socket.on("data", function (bytes) {
//!         console.log("bytes", bytes);
//!     });
//!
//!     socket.on("end", function() {
//!         console.log("Socket received FIN packet and closed connection");
//!     });
//!     socket.on("error", function (error) {
//!         console.log("Socket closed with error", error);
//!     });
//!
//!     socket.on("close", function (with_error) {
//!         if (with_error) {
//!             console.log("Socket closed with result: Err(SomeError)");
//!         } else {
//!             console.log("Socket closed with result: Ok(())");
//!         }
//!     });
//!
//! });
//!
//! server.listen(listenPort);
//!
//! console.log("Listening on:", listenPort);
//! ```
//!

#![deny(warnings)]

extern crate tokio;
extern crate tokio_codec;

use tokio_codec::BytesCodec;
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::codec::Decoder;

use std::env;
use std::net::SocketAddr;

fn main() {
    // Allow passing an address to listen on as the first argument of this
    // program, but otherwise we'll just set up our TCP listener on
    // 127.0.0.1:8080 for connections.
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop, so we pass in a handle
    // to our event loop. After the socket's created we inform that we're ready
    // to go and start accepting connections.
    let socket = TcpListener::bind(&addr).unwrap();
    println!("Listening on: {}", addr);

    // Here we convert the `TcpListener` to a stream of incoming connections
    // with the `incoming` method. We then define how to process each element in
    // the stream with the `for_each` method.
    //
    // This combinator, defined on the `Stream` trait, will allow us to define a
    // computation to happen for all items on the stream (in this case TCP
    // connections made to the server).  The return value of the `for_each`
    // method is itself a future representing processing the entire stream of
    // connections, and ends up being our server.
    let done = socket
        .incoming()
        .map_err(|e| println!("failed to accept socket; error = {:?}", e))
        .for_each(move |socket| {
            // Once we're inside this closure this represents an accepted client
            // from our server. The `socket` is the client connection (similar to
            // how the standard library operates).
            //
            // We're parsing each socket with the `BytesCodec` included in `tokio_io`,
            // and then we `split` each codec into the reader/writer halves.
            //
            // See https://docs.rs/tokio-codec/0.1/src/tokio_codec/bytes_codec.rs.html
            let framed = BytesCodec::new().framed(socket);
            let (_writer, reader) = framed.split();

            let processor = reader
                .for_each(|bytes| {
                    println!("bytes: {:?}", bytes);
                    Ok(())
                })
                // After our copy operation is complete we just print out some helpful
                // information.
                .and_then(|()| {
                    println!("Socket received FIN packet and closed connection");
                    Ok(())
                })
                .or_else(|err| {
                    println!("Socket closed with error: {:?}", err);
                    // We have to return the error to catch it in the next ``.then` call
                    Err(err)
                })
                .then(|result| {
                    println!("Socket closed with result: {:?}", result);
                    Ok(())
                });

            // And this is where much of the magic of this server happens. We
            // crucially want all clients to make progress concurrently, rather than
            // blocking one on completion of another. To achieve this we use the
            // `tokio::spawn` function to execute the work in the background.
            //
            // This function will transfer ownership of the future (`msg` in this
            // case) to the Tokio runtime thread pool that. The thread pool will
            // drive the future to completion.
            //
            // Essentially here we're executing a new task to run concurrently,
            // which will allow all of our clients to be processed concurrently.
            tokio::spawn(processor)
        });

    // And finally now that we've define what our server is, we run it!
    //
    // This starts the Tokio runtime, spawns the server task, and blocks the
    // current thread until all tasks complete execution. Since the `done` task
    // never completes (it just keeps accepting sockets), `tokio::run` blocks
    // forever (until ctrl-c is pressed).
    tokio::run(done);
}
