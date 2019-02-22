extern crate bytes;
extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_threadpool;

use std::io;
use std::net::Shutdown;

use bytes::{BufMut, BytesMut};
use futures::{Future, Sink, Stream};
use tokio::net::{TcpListener, TcpStream};
use tokio_codec::{Decoder, Encoder};
use tokio_io::io::{read, write_all};
use tokio_threadpool::Builder;

pub struct LineCodec;

impl Decoder for LineCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<BytesMut>, io::Error> {
        match buf.iter().position(|&b| b == b'\n') {
            Some(i) => Ok(Some(buf.split_to(i + 1).into())),
            None => Ok(None),
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        if buf.len() == 0 {
            Ok(None)
        } else {
            let amt = buf.len();
            Ok(Some(buf.split_to(amt)))
        }
    }
}

impl Encoder for LineCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn encode(&mut self, item: BytesMut, into: &mut BytesMut) -> io::Result<()> {
        into.put(&item[..]);
        Ok(())
    }
}

#[test]
fn echo() {
    drop(env_logger::try_init());

    let pool = Builder::new().pool_size(1).build();

    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let sender = pool.sender().clone();
    let srv = listener.incoming().for_each(move |socket| {
        let (sink, stream) = LineCodec.framed(socket).split();
        sender
            .spawn(sink.send_all(stream).map(|_| ()).map_err(|_| ()))
            .unwrap();
        Ok(())
    });

    pool.sender()
        .spawn(srv.map_err(|e| panic!("srv error: {}", e)))
        .unwrap();

    let client = TcpStream::connect(&addr);
    let client = client.wait().unwrap();
    let (client, _) = write_all(client, b"a\n").wait().unwrap();
    let (client, buf, amt) = read(client, vec![0; 1024]).wait().unwrap();
    assert_eq!(amt, 2);
    assert_eq!(&buf[..2], b"a\n");

    let (client, _) = write_all(client, b"\n").wait().unwrap();
    let (client, buf, amt) = read(client, buf).wait().unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"\n");

    let (client, _) = write_all(client, b"b").wait().unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let (_client, buf, amt) = read(client, buf).wait().unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"b");
}
