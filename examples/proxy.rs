//! A proxy that forwards data to another server and forwards that server's
//! responses back to clients.
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

extern crate futures;
extern crate futures_cpupool;
extern crate tokio;
extern crate tokio_io;

use std::sync::Arc;
use std::env;
use std::net::{Shutdown, SocketAddr};
use std::io::{self, Read, Write};

use futures::stream::Stream;
use futures::{Future, Poll};
use futures::future::Executor;
use futures_cpupool::CpuPool;
use tokio::net::{TcpListener, TcpStream};
use tokio::reactor::Core;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::io::{copy, shutdown};

fn main() {
    let listen_addr = env::args().nth(1).unwrap_or("127.0.0.1:8081".to_string());
    let listen_addr = listen_addr.parse::<SocketAddr>().unwrap();

    let server_addr = env::args().nth(2).unwrap_or("127.0.0.1:8080".to_string());
    let server_addr = server_addr.parse::<SocketAddr>().unwrap();

    // Create the event loop that will drive this server.
    let mut l = Core::new().unwrap();
    let handle = l.handle();

    let pool = CpuPool::new(1);

    // Create a TCP listener which will listen for incoming connections.
    let socket = TcpListener::bind(&listen_addr, &l.handle()).unwrap();
    println!("Listening on: {}", listen_addr);
    println!("Proxying to: {}", server_addr);

    let done = socket.incoming().for_each(move |(client, client_addr)| {
        let server = TcpStream::connect(&server_addr, &handle);
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
            let client_reader = MyTcpStream(Arc::new(client));
            let client_writer = client_reader.clone();
            let server_reader = MyTcpStream(Arc::new(server));
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
            println!("client at {} wrote {} bytes and received {} bytes",
                     client_addr, from_client, from_server);
        }).map_err(|e| {
            // Don't panic. Maybe the client just disconnected too soon.
            println!("error: {}", e);
        });
        pool.execute(msg).unwrap();

        Ok(())
    });
    l.run(done).unwrap();
}

// This is a custom type used to have a custom implementation of the
// `AsyncWrite::shutdown` method which actually calls `TcpStream::shutdown` to
// notify the remote end that we're done writing.
#[derive(Clone)]
struct MyTcpStream(Arc<TcpStream>);

impl Read for MyTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl Write for MyTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for MyTcpStream {}

impl AsyncWrite for MyTcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        try!(self.0.shutdown(Shutdown::Write));
        Ok(().into())
    }
}
