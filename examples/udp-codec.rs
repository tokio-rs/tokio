extern crate tokio_core;
extern crate env_logger;
extern crate futures;

#[macro_use]
extern crate log;

use std::io;
use std::net::{SocketAddr};
use futures::{future, Future, Stream, Sink};
use tokio_core::io::{CodecUdp};
use tokio_core::net::{UdpSocket};
use tokio_core::reactor::{Core, Timeout};
use std::time::Duration;
use std::str;

/// This is a basic example of leveraging `FramedUdp` to create
/// a simple UDP client and server which speak a custom Protocol. 
/// `FramedUdp` applies a `Codec` to the input and output of an
/// `Evented`

/// Simple Newline based parser, 
/// This is for a connectionless server, it must keep track
/// of the Socket address of the last peer to contact it
/// so that it can respond back. 
/// In the real world, one would probably
/// want an associative of remote peers to their state
pub struct LineCodec {
    addr : Option<SocketAddr>
}

impl CodecUdp for LineCodec {
    type In = Vec<u8>;
    type Out = Vec<u8>;

    fn decode(&mut self, addr : &SocketAddr, buf: &[u8]) -> Result<Option<Self::In>, io::Error> {
        trace!("decoding {} - {}", str::from_utf8(buf).unwrap(), addr);
        self.addr = Some(*addr);
        match buf.iter().position(|&b| b == b'\n') {
            Some(i) => Ok(Some(buf[.. i].into())),
            None => Ok(None),
        }
    }
    
    fn encode(&mut self, item: &Vec<u8>, into: &mut Vec<u8>) -> SocketAddr {
        trace!("encoding {}", str::from_utf8(item.as_slice()).unwrap());
        into.extend_from_slice(item.as_slice());
        into.push('\n' as u8);
        
        self.addr.unwrap()
    }
}

fn main() {
    drop(env_logger::init());

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    //create the line codec parser for each 
    let srvcodec = LineCodec { addr : None };
    let clicodec = LineCodec { addr : None };

    let srvaddr : SocketAddr = "127.0.0.1:31999".parse().unwrap();
    let clientaddr : SocketAddr = "127.0.0.1:32000".parse().unwrap();

    //We bind each socket to a specific port
    let server = UdpSocket::bind(&srvaddr, &handle).unwrap();
    let client = UdpSocket::bind(&clientaddr, &handle).unwrap();

    //start things off by sending a ping from the client to the server
    //This doesn't go through the codec to encode the message, but rather
    //it sends raw data with the send_dgram future
    {
        let job = client.send_dgram(b"PING\n", &srvaddr);
        core.run(job).unwrap();
    }

    //We create a FramedUdp instance, which associates a socket
    //with a codec. We then immediate split that into the 
    //receiving side `Stream` and the writing side `Sink`
    let (srvstream, srvsink) = server.framed(srvcodec).split();

    //`Stream::fold` runs once per every received datagram.
    //Note that we pass srvsink into fold, so that it can be
    //supplied to every iteration. The reason for this is
    //sink.send moves itself into `send` and then returns itself
    let srvloop = srvstream.fold(srvsink, move |sink, buf| { 
        println!("{}", str::from_utf8(buf.as_slice()).unwrap());
        sink.send(b"PONG".to_vec())
    }).map(|_| ());
    
    //We create another FramedUdp instance, this time for the client socket
    let (clistream, clisink) = client.framed(clicodec).split();

    //And another infinite iteration
    let cliloop = clistream.fold(clisink, move |sink, buf| { 
            println!("{}", str::from_utf8(buf.as_slice()).unwrap());
            sink.send(b"PING".to_vec())
        }).map(|_| ());

    let timeout = Timeout::new(Duration::from_millis(500), &handle).unwrap();

    //`select_all` takes an `Iterable` of `Future` and returns a future itself
    //This future waits until the first `Future` completes, it then returns
    //that result.
    let wait = future::select_all(vec![timeout.boxed(), srvloop.boxed(), cliloop.boxed()]);

    //Now we instruct `reactor::Core` to iterate, processing events until its future, `SelectAll`
    //has completed
    if let Err(e) = core.run(wait) {
        error!("{}", e.0);
    }
}
