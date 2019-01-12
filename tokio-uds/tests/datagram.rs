#![cfg(unix)]

extern crate bytes;
extern crate futures;
extern crate tempfile;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_uds;

use tokio_uds::*;

use std::str;

use bytes::BytesMut;

use tokio::io;
use tokio::runtime::current_thread::Runtime;

use tokio_codec::{Decoder, Encoder};

use futures::{Future, Sink, Stream};

struct StringDatagramCodec;

/// A codec to decode datagrams from a unix domain socket as utf-8 text messages.
impl Encoder for StringDatagramCodec {
    type Item = String;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&item.into_bytes());
        Ok(())
    }
}

/// A codec to decode datagrams from a unix domain socket as utf-8 text messages.
impl Decoder for StringDatagramCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let decoded = str::from_utf8(buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .to_string();

        Ok(Some(decoded))
    }
}

#[test]
fn framed_echo() {
    let dir = tempfile::tempdir().unwrap();
    let server_path = dir.path().join("server.sock");
    let client_path = dir.path().join("client.sock");

    let mut rt = Runtime::new().unwrap();

    {
        let socket = UnixDatagram::bind(&server_path).unwrap();
        let server = UnixDatagramFramed::new(socket, StringDatagramCodec);

        let (sink, stream) = server.split();

        let echo_stream = stream.map(|(msg, addr)| (msg, addr.as_pathname().unwrap().to_path_buf()));

        // spawn echo server
        rt.spawn(echo_stream.forward(sink)
                 .map_err(|e| panic!("err={:?}", e))
                 .map(|_| ()));
    }

    {
        let socket = UnixDatagram::bind(&client_path).unwrap();
        let client = UnixDatagramFramed::new(socket, StringDatagramCodec);

        let (sink, stream) = client.split();

        rt.block_on(sink.send(("ECHO".to_string(), server_path))).unwrap();

        let response = rt.block_on(stream.take(1).collect()).unwrap();
        assert_eq!(response[0].0, "ECHO");
    }
}
