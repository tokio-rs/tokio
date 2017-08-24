extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate bytes;

use std::io;
use std::net::Shutdown;

use bytes::{BytesMut, BufMut};
<<<<<<< 822d9f84453ac74646d1b8d8c87395f75bcb7604
use futures::{Future, Stream, Sink};
use tokio::net::{TcpListener, TcpStream};
use tokio::reactor::Core;
=======
use futures::prelude::*;
use futures::thread::TaskRunner;
use tokio::net::{TcpListener, TcpStream};
use tokio_io::AsyncRead;
>>>>>>> Update as the `tokio` crate
use tokio_io::codec::{Encoder, Decoder};
use tokio_io::io::{write_all, read};

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

    let mut tasks = TaskRunner::new();
    let spawner = tasks.spawner();
    let listener = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = listener.incoming().for_each(move |(socket, _)| {
        let (sink, stream) = socket.framed(LineCodec).split();
        spawner.spawn(sink.send_all(stream).map(|_| ()).map_err(|_| ()));
        Ok(())
    });

    let spawner = tasks.spawner();
    spawner.spawn(srv.map_err(|e| panic!("srv error: {}", e)));

    let client = TcpStream::connect(&addr);
    let client = tasks.block_until(client).unwrap();
    let (client, _) = tasks.block_until(write_all(client, b"a\n")).unwrap();
    let (client, buf, amt) = tasks.block_until(read(client, vec![0; 1024])).unwrap();
    assert_eq!(amt, 2);
    assert_eq!(&buf[..2], b"a\n");

    let (client, _) = tasks.block_until(write_all(client, b"\n")).unwrap();
    let (client, buf, amt) = tasks.block_until(read(client, buf)).unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"\n");

    let (client, _) = tasks.block_until(write_all(client, b"b")).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let (_client, buf, amt) = tasks.block_until(read(client, buf)).unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"b");
}
