extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use std::io;
use std::net::Shutdown;

use futures::{Future, Async, Poll};
use futures::stream::Stream;
use tokio_core::io::{FramedIo, write_all, read};
use tokio_core::easy::{Parse, Serialize, EasyFramed, EasyBuf};
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::reactor::Core;

pub struct LineParser;
pub struct LineSerialize;

impl Parse for LineParser {
    type Out = EasyBuf;

    fn parse(&mut self, buf: &mut EasyBuf) -> Poll<EasyBuf, io::Error> {
        match buf.as_slice().iter().position(|&b| b == b'\n') {
            Some(i) => Ok(buf.drain_to(i + 1).into()),
            None => Ok(Async::NotReady),
        }
    }

    fn done(&mut self, buf: &mut EasyBuf) -> io::Result<EasyBuf> {
        let amt = buf.len();
        Ok(buf.drain_to(amt))
    }
}

impl Serialize for LineSerialize {
    type In = EasyBuf;

    fn serialize(&mut self, msg: EasyBuf, into: &mut Vec<u8>) {
        into.extend_from_slice(msg.as_slice());
    }
}

pub struct EchoFramed<T> {
    inner: T,
    eof: bool,
}

impl<T, U> Future for EchoFramed<T>
    where T: FramedIo<In = U, Out = Option<U>>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        // Try to write out any buffered messages if we have them.
        if self.inner.flush().expect("flush error").is_not_ready() {
            return Ok(Async::NotReady)
        }

        // Wait until we can simultaneously read and write a message
        while !self.eof &&
              self.inner.poll_read().is_ready() &&
              self.inner.poll_write().is_ready() {
            let frame = match self.inner.read() {
                Ok(Async::Ready(Some(frame))) => frame,
                Ok(Async::Ready(None)) => {
                    self.eof = true;
                    break
                }
                Ok(Async::NotReady) => break,
                Err(e) => panic!("error in read: {}", e),
            };

            match self.inner.write(frame) {
                Ok(Async::Ready(())) => {}
                Ok(Async::NotReady) => break,
                Err(e) => panic!("error in write: {}", e),
            }
        }

        // If we wrote some frames try to flush again. Ignore whether this is
        // ready to finish or not as we're going to continue to return NotReady
        if self.inner.flush().expect("flush error").is_ready() && self.eof {
            Ok(().into())
        } else {
            Ok(Async::NotReady)
        }
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
        let framed = EasyFramed::new(socket, LineParser, LineSerialize);
        handle.spawn(EchoFramed { inner: framed, eof: false });
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

    let (client, _) = core.run(write_all(client, b"\nb\n")).unwrap();
    let (client, buf, amt) = core.run(read(client, buf)).unwrap();
    assert_eq!(amt, 3);
    assert_eq!(&buf[..3], b"\nb\n");

    let (client, _) = core.run(write_all(client, b"b")).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let (_client, buf, amt) = core.run(read(client, buf)).unwrap();
    assert_eq!(amt, 1);
    assert_eq!(&buf[..1], b"b");
}
