//! A multithreaded version of an echo server
//!
//! This server implements the same functionality as the `echo` example, except
//! that this example will use all cores of the machine to do I/O instead of
//! just one. This examples works by having the main thread using blocking I/O
//! and shipping accepted sockets to worker threads in a round-robin fashion.
//!
//! To see this server in action, you can run this in one terminal:
//!
//!     cargo run --example echo-threads
//!
//! and in another terminal you can run:
//!
//!     cargo run --example connect 127.0.0.1:8080

extern crate futures;
extern crate futures_cpupool;
extern crate num_cpus;
extern crate tokio;
extern crate tokio_io;

use std::env;
use std::net::SocketAddr;
use std::thread;

use futures::prelude::*;
use futures::future::{self, Executor};
use futures::sync::mpsc;
use futures_cpupool::CpuPool;
use tokio_io::AsyncRead;
use tokio_io::io::copy;
use tokio::net::{TcpStream, TcpListener};

fn main() {
    // First argument, the address to bind
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    // Second argument, the number of threads we'll be using
    let num_threads = env::args().nth(2).and_then(|s| s.parse().ok())
        .unwrap_or(num_cpus::get());

    let listener = TcpListener::bind(&addr).expect("failed to bind");
    println!("Listening on: {}", addr);

    // Spin up our worker threads, creating a channel routing to each worker
    // thread that we'll use below.
    let mut channels = Vec::new();
    for _ in 0..num_threads {
        let (tx, rx) = mpsc::unbounded();
        channels.push(tx);
        thread::spawn(|| worker(rx));
    }

    // Infinitely accept sockets from our `TcpListener`.  Each socket is then
    // shipped round-robin to a particular thread which will associate the
    // socket with the corresponding event loop and process the connection.
    let mut next = 0;
    let srv = listener.incoming().for_each(|socket| {
        channels[next].unbounded_send(socket).expect("worker thread died");
        next = (next + 1) % channels.len();
        Ok(())
    });
    future::blocking(srv).wait().unwrap();
}

fn worker(rx: mpsc::UnboundedReceiver<TcpStream>) {
    let pool = CpuPool::new(1);

    let done = rx.for_each(move |socket| {
        let addr = socket.peer_addr().expect("failed to get remote address");

        // Like the single-threaded `echo` example we split the socket halves
        // and use the `copy` helper to ship bytes back and forth. Afterwards we
        // spawn the task to run concurrently on this thread, and then print out
        // what happened afterwards
        let (reader, writer) = socket.split();
        let amt = copy(reader, writer);
        let msg = amt.then(move |result| {
            match result {
                Ok((amt, _, _)) => println!("wrote {} bytes to {}", amt, addr),
                Err(e) => println!("error on {}: {}", addr, e),
            }

            Ok(())
        });
        pool.execute(msg).unwrap();

        Ok(())
    });
    future::blocking(done).wait().unwrap();
}
