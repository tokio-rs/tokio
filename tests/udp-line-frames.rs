extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use std::io;
use std::net::{SocketAddr};
use futures::{future, Future, Stream, Sink, IntoFuture};
use tokio_core::io::{write_all, read, FramedUdp, CodecUdp, Io};
use tokio_core::net::{UdpSocket};
use tokio_core::reactor::{Core, Timeout};
use std::time::Duration;
use std::str;

pub struct LineCodec {
    addr : Option<SocketAddr>
}

impl CodecUdp for LineCodec {
    type In = Vec<u8>;
    type Out = Vec<u8>;

    fn decode(&mut self, addr : &SocketAddr, buf: &mut Vec<u8>) -> Result<Option<Self::In>, io::Error> {
        self.addr = Some(*addr);
        match buf.as_slice().iter().position(|&b| b == b'\n') {
            Some(i) => Ok(Some(buf[.. i + 1].into())),
            None => Ok(None),
        }
    }
    
    fn encode(&mut self, item: Vec<u8>, into: &mut Vec<u8>) -> SocketAddr {
        into.extend_from_slice(item.as_slice());
        into.push('\n' as u8);
        
        self.addr.unwrap()
    }
}

#[test]
fn echo() {
    drop(env_logger::init());

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let srvcodec = LineCodec { addr : None };
    let clicodec = LineCodec { addr : None };

    let srvaddr : SocketAddr = "127.0.0.1:31999".parse().unwrap();
    let clientaddr : SocketAddr = "127.0.0.1:32000".parse().unwrap();

    let server = UdpSocket::bind(&srvaddr, &handle).unwrap();
    let client = UdpSocket::bind(&clientaddr, &handle).unwrap();

    let job = client.send_to(b"PING", &srvaddr);
    let _ = core.run(job.into_future()).unwrap();

    let (srvstream, srvsink) = server.framed(srvcodec).split();
    let srvloop = srvstream.for_each(move |buf| { 
        println!("{}", str::from_utf8(buf.as_slice()).unwrap());
        srvsink.send(b"PONG".to_vec()).map(|_| ()).wait()
    });
    
    let (clistream, clisink) = client.framed(clicodec).split();
    let cliloop = clistream.for_each(move |buf| { 
            println!("{}", str::from_utf8(buf.as_slice()).unwrap());
            clisink.send(b"PING".to_vec()).map(|_| ()).wait()
        });

    let timeout = Timeout::new(Duration::from_millis(500), &handle).unwrap();

    let wait = future::select_all(vec![timeout.boxed(), srvloop.boxed(), cliloop.boxed()]); 
    core.run(wait);

}
