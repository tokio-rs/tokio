//! An example of offloading work to a thread pool instead of doing work on the
//! main event loop.
//!
//! In this example the server will act as a form of echo server except that
//! it'll echo back gzip-compressed data. Each connected client will have the
//! data written streamed back as the compressed version is available, and all
//! compressing will occur on a thread pool rather than the main event loop.
//!
//! You can preview this example with in one terminal:
//!
//!     cargo run --example compress
//!
//! and in another terminal;
//!
//!     echo test | cargo run --example connect 127.0.0.1:8080 | gunzip
//!
//! The latter command will need to be tweaked for non-unix-like shells, but
//! you can also redirect the stdout of the `connect` program to a file
//! and then decompress that.

extern crate futures;
extern crate futures_cpupool;
extern crate flate2;
extern crate tokio;
extern crate tokio_io;

use std::io;
use std::env;
use std::net::SocketAddr;

use futures::{Future, Stream, Poll};
use futures::future::{self, Executor};
use futures_cpupool::CpuPool;
use tokio::net::{TcpListener, TcpStream};
use tokio_io::{AsyncRead, AsyncWrite};
use flate2::write::GzEncoder;

fn main() {
    // As with many other examples, parse our CLI arguments and prepare the
    // reactor.
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();
    let socket = TcpListener::bind(&addr).unwrap();
    println!("Listening on: {}", addr);

    // This is where we're going to offload our computationally heavy work
    // (compressing) to. Here we just use a convenience constructor to create a
    // pool of threads equal to the number of CPUs we have.
    let pool = CpuPool::new_num_cpus();

    // The compress logic will happen in the function below, but everything's
    // still a future! Each client is spawned to concurrently get processed.
    let server = socket.incoming().for_each(move |socket| {
        let addr = socket.peer_addr().unwrap();
        pool.execute(compress(socket, &pool).then(move |result| {
            match result {
                Ok((r, w)) => println!("{}: compressed {} bytes to {}", addr, r, w),
                Err(e) => println!("{}: failed when compressing: {}", addr, e),
            }
            Ok(())
        })).unwrap();
        Ok(())
    });

    future::blocking(server).wait().unwrap();
}

/// The main workhorse of this example. This'll compress all data read from
/// `socket` on the `pool` provided, writing it back out to `socket` as it's
/// available.
fn compress(socket: TcpStream, pool: &CpuPool)
    -> Box<Future<Item = (u64, u64), Error = io::Error> + Send>
{
    use tokio_io::io;

    // The general interface that `CpuPool` provides is that we'll *spawn a
    // future* onto it. All execution of the future will occur on the `CpuPool`
    // and we'll get back a handle representing the completed value of the
    // future. In essence it's our job here to create a future that represents
    // compressing `socket`, and then we'll simply spawn it at the very end.
    //
    // Here we exploit the fact that `TcpStream` itself is `Send` in this
    // function as well. That is, we can read/write the TCP stream on any
    // thread, and we'll get notifications about it being ready from the reactor
    // thread.
    //
    // Otherwise this is the same as the echo server except that after splitting
    // we apply some encoding to one side, followed by a `shutdown` when we're
    // done to ensure that all gz footers are written.
    let (read, write) = socket.split();
    let write = Count { io: write, amt: 0 };
    let write = GzEncoder::new(write, flate2::Compression::best());
    let process = io::copy(read, write).and_then(|(amt, _read, write)| {
        io::shutdown(write).map(move |io| (amt, io.get_ref().amt))
    });

    // Spawn the future so is executes entirely on the thread pool here
    Box::new(pool.spawn(process))
}

struct Count<T> {
    io: T,
    amt: u64,
}

impl<T: io::Write> io::Write for Count<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.io.write(buf)?;
        self.amt += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl<T: AsyncWrite> AsyncWrite for Count<T> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.io.shutdown()
    }
}
