extern crate env_logger;
extern crate futures;
extern crate futures_cpupool;
extern crate tokio;
extern crate tokio_io;
extern crate bytes;

use std::io;
use std::net::Shutdown;

use bytes::{BytesMut, BufMut};
use futures::{Future, Stream, Sink};
use futures::future::Executor;
use futures_cpupool::CpuPool;
use tokio::net::{TcpListener, TcpStream};
use tokio_io::codec::{Encoder, Decoder};
use tokio_io::io::{write_all, read};
use tokio_io::AsyncRead;

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
    drop(env_logger::init());

    let pool = CpuPool::new(1);

    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let pool_inner = pool.clone();
    let srv = listener.incoming().for_each(move |(socket, _)| {
        let (sink, stream) = socket.framed(LineCodec).split();
        pool_inner.execute(sink.send_all(stream).map(|_| ()).map_err(|_| ())).unwrap();
        Ok(())
    });

    pool.execute(srv.map_err(|e| panic!("srv error: {}", e))).unwrap();

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
