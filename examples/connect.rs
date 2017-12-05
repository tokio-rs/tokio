//! An example of hooking up stdin/stdout to either a TCP or UDP stream.
//!
//! This example will connect to a socket address specified in the argument list
//! and then forward all data read on stdin to the server, printing out all data
//! received on stdout. An optional `--udp` argument can be passed to specify
//! that the connection should be made over UDP instead of TCP, translating each
//! line entered on stdin to a UDP packet to be sent to the remote address.
//!
//! Note that this is not currently optimized for performance, especially
//! around buffer management. Rather it's intended to show an example of
//! working with a client.
//!
//! This example can be quite useful when interacting with the other examples in
//! this repository! Many of them recommend running this as a simple "hook up
//! stdin/stdout to a server" to get up and running.

extern crate futures;
extern crate futures_cpupool;
extern crate tokio;
extern crate tokio_io;
extern crate bytes;

use std::env;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::thread;

use futures::sync::mpsc;
use futures::{Sink, Future, Stream};
use futures_cpupool::CpuPool;
use tokio::reactor::Handle;

fn main() {
    // Determine if we're going to run in TCP or UDP mode
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let tcp = match args.iter().position(|a| a == "--udp") {
        Some(i) => {
            args.remove(i);
            false
        }
        None => true,
    };

    // Parse what address we're going to connect to
    let addr = args.first().unwrap_or_else(|| {
        panic!("this program requires at least one argument")
    });
    let addr = addr.parse::<SocketAddr>().unwrap();

    let handle = Handle::default();

    let pool = CpuPool::new(1);

    // Right now Tokio doesn't support a handle to stdin running on the event
    // loop, so we farm out that work to a separate thread. This thread will
    // read data (with blocking I/O) from stdin and then send it to the event
    // loop over a standard futures channel.
    let (stdin_tx, stdin_rx) = mpsc::channel(0);
    thread::spawn(|| read_stdin(stdin_tx));
    let stdin_rx = stdin_rx.map_err(|_| panic!()); // errors not possible on rx

    // Now that we've got our stdin read we either set up our TCP connection or
    // our UDP connection to get a stream of bytes we're going to emit to
    // stdout.
    let stdout = if tcp {
        tcp::connect(&addr, &handle, &pool, Box::new(stdin_rx))
    } else {
        udp::connect(&addr, &handle, &pool, Box::new(stdin_rx))
    };

    // And now with our stream of bytes to write to stdout, we execute that in
    // the event loop! Note that this is doing blocking I/O to emit data to
    // stdout, and in general it's a no-no to do that sort of work on the event
    // loop. In this case, though, we know it's ok as the event loop isn't
    // otherwise running anything useful.
    let mut out = io::stdout();
    stdout.for_each(|chunk| {
        out.write_all(&chunk)
    }).wait().unwrap();
}

mod tcp {
    use std::io;
    use std::net::SocketAddr;

    use bytes::{BufMut, BytesMut};
    use futures::{Future, Stream};
    use futures::future::Executor;
    use futures_cpupool::CpuPool;
    use tokio::net::TcpStream;
    use tokio::reactor::Handle;
    use tokio_io::AsyncRead;
    use tokio_io::codec::{Encoder, Decoder};

    pub fn connect(addr: &SocketAddr,
                   handle: &Handle,
                   pool: &CpuPool,
                   stdin: Box<Stream<Item = Vec<u8>, Error = io::Error> + Send>)
        -> Box<Stream<Item = BytesMut, Error = io::Error>>
    {
        let tcp = TcpStream::connect(addr, handle);
        let pool = pool.clone();

        // After the TCP connection has been established, we set up our client
        // to start forwarding data.
        //
        // First we use the `Io::framed` method with a simple implementation of
        // a `Codec` (listed below) that just ships bytes around. We then split
        // that in two to work with the stream and sink separately.
        //
        // Half of the work we're going to do is to take all data we receive on
        // `stdin` and send that along the TCP stream (`sink`). The second half
        // is to take all the data we receive (`stream`) and then write that to
        // stdout. We'll be passing this handle back out from this method.
        //
        // You'll also note that we *spawn* the work to read stdin and write it
        // to the TCP stream. This is done to ensure that happens concurrently
        // with us reading data from the stream.
        Box::new(tcp.map(move |stream| {
            let (sink, stream) = stream.framed(Bytes).split();
            pool.execute(stdin.forward(sink).then(|result| {
                if let Err(e) = result {
                    panic!("failed to write to socket: {}", e)
                }
                Ok(())
            })).unwrap();
            stream
        }).flatten_stream())
    }

    /// A simple `Codec` implementation that just ships bytes around.
    ///
    /// This type is used for "framing" a TCP stream of bytes but it's really
    /// just a convenient method for us to work with streams/sinks for now.
    /// This'll just take any data read and interpret it as a "frame" and
    /// conversely just shove data into the output location without looking at
    /// it.
    struct Bytes;

    impl Decoder for Bytes {
        type Item = BytesMut;
        type Error = io::Error;

        fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            if buf.len() > 0 {
                let len = buf.len();
                Ok(Some(buf.split_to(len)))
            } else {
                Ok(None)
            }
        }

        fn decode_eof(&mut self, buf: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            self.decode(buf)
        }
    }

    impl Encoder for Bytes {
        type Item = Vec<u8>;
        type Error = io::Error;

        fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> io::Result<()> {
            buf.put(&data[..]);
            Ok(())
        }
    }
}

mod udp {
    use std::io;
    use std::net::SocketAddr;

    use bytes::BytesMut;
    use futures::{Future, Stream};
    use futures::future::Executor;
    use futures_cpupool::CpuPool;
    use tokio::net::{UdpCodec, UdpSocket};
    use tokio::reactor::Handle;

    pub fn connect(&addr: &SocketAddr,
                   handle: &Handle,
                   pool: &CpuPool,
                   stdin: Box<Stream<Item = Vec<u8>, Error = io::Error> + Send>)
        -> Box<Stream<Item = BytesMut, Error = io::Error>>
    {
        // We'll bind our UDP socket to a local IP/port, but for now we
        // basically let the OS pick both of those.
        let addr_to_bind = if addr.ip().is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let udp = UdpSocket::bind(&addr_to_bind, handle)
            .expect("failed to bind socket");

        // Like above with TCP we use an instance of `UdpCodec` to transform
        // this UDP socket into a framed sink/stream which operates over
        // discrete values. In this case we're working with *pairs* of socket
        // addresses and byte buffers.
        let (sink, stream) = udp.framed(Bytes).split();

        // All bytes from `stdin` will go to the `addr` specified in our
        // argument list. Like with TCP this is spawned concurrently
        pool.execute(stdin.map(move |chunk| {
            (addr, chunk)
        }).forward(sink).then(|result| {
            if let Err(e) = result {
                panic!("failed to write to socket: {}", e)
            }
            Ok(())
        })).unwrap();

        // With UDP we could receive data from any source, so filter out
        // anything coming from a different address
        Box::new(stream.filter_map(move |(src, chunk)| {
            if src == addr {
                Some(chunk.into())
            } else {
                None
            }
        }))
    }

    struct Bytes;

    impl UdpCodec for Bytes {
        type In = (SocketAddr, Vec<u8>);
        type Out = (SocketAddr, Vec<u8>);

        fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
            Ok((*addr, buf.to_vec()))
        }

        fn encode(&mut self, (addr, buf): Self::Out, into: &mut Vec<u8>) -> SocketAddr {
            into.extend(buf);
            addr
        }
    }
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
fn read_stdin(mut tx: mpsc::Sender<Vec<u8>>) {
    let mut stdin = io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf) {
            Err(_) |
            Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        tx = match tx.send(buf).wait() {
            Ok(tx) => tx,
            Err(_) => break,
        };
    }
}
