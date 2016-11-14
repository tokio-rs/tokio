extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use std::io;
use std::net::Shutdown;

use futures::{Future, Stream, Sink};
use tokio_core::io::{write_all, read, Codec, EasyBuf, Io};
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::reactor::Core;

pub struct LineCodec;

impl Codec for LineCodec {
    type In = EasyBuf;
    type Out = EasyBuf;

    fn decode(&mut self, buf: &mut EasyBuf) -> Result<Option<EasyBuf>, io::Error> {
        match buf.as_slice().iter().position(|&b| b == b'\n') {
            Some(i) => Ok(Some(buf.drain_to(i + 1).into())),
            None => Ok(None),
        }
    }

    fn decode_eof(&mut self, buf: &mut EasyBuf) -> io::Result<EasyBuf> {
        let amt = buf.len();
        Ok(buf.drain_to(amt))
    }

    fn encode(&mut self, item: EasyBuf, into: &mut Vec<u8>) {
        into.extend_from_slice(item.as_slice());
    }
}

#[test]
fn echo() {
    drop(env_logger::init());

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap(), &handle).unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = listener.incoming().for_each(move |(socket, _)| {
        let (stream, sink) = socket.framed(LineCodec).split();
        handle.spawn(sink.send_all(stream).map(|_| ()).map_err(|_| ()));
        Ok(())
    });

    let handle = core.handle();
    handle.spawn(srv.map_err(|e| panic!("srv error: {}", e)));

    let client = TcpStream::connect(&addr, &handle);
    let client = core.run(client).unwrap();
    let (client, _) = core.run(write_all(client, b"a\n")).unwrap();
    let (client, buf, amt) = core.run(read(client, vec![0; 1024])).unwrap();
    assert_eq!(amt, 2);
    assert_eq!(&buf[..2], b"a\n");

    let (client, _) = core.run(write_all(client, b"\n")).unwrap();
    let (client, buf, amt) = core.run(read(client, buf)).unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"\n");

    let (client, _) = core.run(write_all(client, b"b")).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let (_client, buf, amt) = core.run(read(client, buf)).unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"b");
}
