//! An "hello world" echo server with tokio-core
//!
//! This server will create a TCP listener, accept connections in a loop, and
//! simply write back everything that's read off of each TCP connection. Each
//! TCP connection is processed concurrently with all other TCP connections, and
//! each connection will have its own buffer that it's reading in/out of.
//!
//! To see this server in action, you can run this in one terminal:
//!
//!     cargo run --example echo
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect 127.0.0.1:8080
//!
//! Each line you type in to the `connect` terminal should be echo'd back to
//! you! If you open up multiple terminals running the `connect` example you
//! should be able to see them all make progress simultaneously.

extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use std::env;
use std::net::SocketAddr;

use futures::Future;
use futures::stream::Stream;
use tokio_io::AsyncRead;
use tokio_io::io::copy;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;

fn main() {
    // Allow passing an address to listen on as the first argument of this
    // program, but otherwise we'll just set up our TCP listener on
    // 127.0.0.1:8080 for connections.
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // First up we'll create the event loop that's going to drive this server.
    // This is done by creating an instance of the `Core` type, tokio-core's
    // event loop. Most functions in tokio-core return an `io::Result`, and
    // `Core::new` is no exception. For this example, though, we're mostly just
    // ignoring errors, so we unwrap the return value.
    //
    // After the event loop is created we acquire a handle to it through the
    // `handle` method. With this handle we'll then later be able to create I/O
    // objects and spawn futures.
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop, so we pass in a handle
    // to our event loop. After the socket's created we inform that we're ready
    // to go and start accepting connections.
    let socket = TcpListener::bind(&addr, &handle).unwrap();
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
    let done = socket.incoming().for_each(move |(socket, addr)| {

        // Once we're inside this closure this represents an accepted client
        // from our server. The `socket` is the client connection and `addr` is
        // the remote address of the client (similar to how the standard library
        // operates).
        //
        // We just want to copy all data read from the socket back onto the
        // socket itself (e.g. "echo"). We can use the standard `io::copy`
        // combinator in the `tokio-core` crate to do precisely this!
        //
        // The `copy` function takes two arguments, where to read from and where
        // to write to. We only have one argument, though, with `socket`.
        // Luckily there's a method, `Io::split`, which will split an Read/Write
        // stream into its two halves. This operation allows us to work with
        // each stream independently, such as pass them as two arguments to the
        // `copy` function.
        //
        // The `copy` function then returns a future, and this future will be
        // resolved when the copying operation is complete, resolving to the
        // amount of data that was copied.
        let (reader, writer) = socket.split();
        let amt = copy(reader, writer);

        // After our copy operation is complete we just print out some helpful
        // information.
        let msg = amt.then(move |result| {
            match result {
                Ok((amt, _, _)) => println!("wrote {} bytes to {}", amt, addr),
                Err(e) => println!("error on {}: {}", addr, e),
            }

            Ok(())
        });

        // And this is where much of the magic of this server happens. We
        // crucially want all clients to make progress concurrently, rather than
        // blocking one on completion of another. To achieve this we use the
        // `spawn` function on `Handle` to essentially execute some work in the
        // background.
        //
        // This function will transfer ownership of the future (`msg` in this
        // case) to the event loop that `handle` points to. The event loop will
        // then drive the future to completion.
        //
        // Essentially here we're spawning a new task to run concurrently, which
        // will allow all of our clients to be processed concurrently.
        handle.spawn(msg);

        Ok(())
    });

    // And finally now that we've define what our server is, we run it! We
    // didn't actually do much I/O up to this point and this `Core::run` method
    // is responsible for driving the entire server to completion.
    //
    // The `run` method will return the result of the future that it's running,
    // but in our case the `done` future won't ever finish because a TCP
    // listener is never done accepting clients. That basically just means that
    // we're going to be running the server until it's killed (e.g. ctrl-c).
    core.run(done).unwrap();
}
