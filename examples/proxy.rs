//! A proxy that forwards data to another server and forwards that server's
//! responses back to clients.
//!
//! Because the Tokio runtime uses a thread pool, each TCP connection is
//! processed concurrently with all other TCP connections across multiple
//! threads.
//!
//! You can showcase this by running this in one terminal:
//!
//!     cargo run --example proxy
//!
//! This in another terminal
//!
//!     cargo run --example echo
//!
//! And finally this in another terminal
//!
//!     cargo run --example connect 127.0.0.1:8081
//!
//! This final terminal will connect to our proxy, which will in turn connect to
//! the echo server, and you'll be able to see data flowing between them.

#![deny(warnings)]

extern crate tokio;

use std::sync::{Arc, Mutex};
use std::env;
use std::net::{Shutdown, SocketAddr};
use std::io::{self, Read, Write};

use tokio::io::{copy, shutdown};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

fn main() {
    let listen_addr = env::args().nth(1).unwrap_or("127.0.0.1:8081".to_string());
    let listen_addr = listen_addr.parse::<SocketAddr>().unwrap();

    let server_addr = env::args().nth(2).unwrap_or("127.0.0.1:8080".to_string());
    let server_addr = server_addr.parse::<SocketAddr>().unwrap();

    // Create a TCP listener which will listen for incoming connections.
    let socket = TcpListener::bind(&listen_addr).unwrap();
    println!("Listening on: {}", listen_addr);
    println!("Proxying to: {}", server_addr);

    let done = socket.incoming()
        .map_err(|e| println!("error accepting socket; error = {:?}", e))
        .for_each(move |client| {
            let server = TcpStream::connect(&server_addr);
            let amounts = server.and_then(move |server| {
                // Create separate read/write handles for the TCP clients that we're
                // proxying data between. Note that typically you'd use
                // `AsyncRead::split` for this operation, but we want our writer
                // handles to have a custom implementation of `shutdown` which
                // actually calls `TcpStream::shutdown` to ensure that EOF is
                // transmitted properly across the proxied connection.
                //
                // As a result, we wrap up our client/server manually in arcs and
                // use the impls below on our custom `MyTcpStream` type.
                let client_reader = MyTcpStream(Arc::new(Mutex::new(client)));
                let client_writer = client_reader.clone();
                let server_reader = MyTcpStream(Arc::new(Mutex::new(server)));
                let server_writer = server_reader.clone();

                // Copy the data (in parallel) between the client and the server.
                // After the copy is done we indicate to the remote side that we've
                // finished by shutting down the connection.
                let client_to_server = copy(client_reader, server_writer)
                    .and_then(|(n, _, server_writer)| {
                        shutdown(server_writer).map(move |_| n)
                    });

                let server_to_client = copy(server_reader, client_writer)
                    .and_then(|(n, _, client_writer)| {
                        shutdown(client_writer).map(move |_| n)
                    });

                client_to_server.join(server_to_client)
            });

            let msg = amounts.map(move |(from_client, from_server)| {
                println!("client wrote {} bytes and received {} bytes",
                         from_client, from_server);
            }).map_err(|e| {
                // Don't panic. Maybe the client just disconnected too soon.
                println!("error: {}", e);
            });

            tokio::spawn(msg);

            Ok(())
        });

    tokio::run(done);
}

// This is a custom type used to have a custom implementation of the
// `AsyncWrite::shutdown` method which actually calls `TcpStream::shutdown` to
// notify the remote end that we're done writing.
#[derive(Clone)]
struct MyTcpStream(Arc<Mutex<TcpStream>>);

impl Read for MyTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.lock().unwrap().read(buf)
    }
}

impl Write for MyTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for MyTcpStream {}

impl AsyncWrite for MyTcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        try!(self.0.lock().unwrap().shutdown(Shutdown::Write));
        Ok(().into())
    }
}
