#![feature(async_await)]
#![warn(rust_2018_idioms)]

use tokio::prelude::*;
use tokio_codec::{Decoder, Encoder, Framed, FramedParts};
use tokio_test::assert_ok;

use bytes::{Buf, BufMut, BytesMut, IntoBuf};
use std::io::{self, Read};
use std::pin::Pin;
use std::task::{Context, Poll};

const INITIAL_CAPACITY: usize = 8 * 1024;

/// Encode and decode u32 values.
struct U32Codec;

impl Decoder for U32Codec {
    type Item = u32;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<u32>> {
        if buf.len() < 4 {
            return Ok(None);
        }

        let n = buf.split_to(4).into_buf().get_u32_be();
        Ok(Some(n))
    }
}

impl Encoder for U32Codec {
    type Item = u32;
    type Error = io::Error;

    fn encode(&mut self, item: u32, dst: &mut BytesMut) -> io::Result<()> {
        // Reserve space
        dst.reserve(4);
        dst.put_u32_be(item);
        Ok(())
    }
}

/// This value should never be used
struct DontReadIntoThis;

impl Read for DontReadIntoThis {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Read into something you weren't supposed to.",
        ))
    }
}

impl AsyncRead for DontReadIntoThis {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        unreachable!()
    }
}

#[tokio::test]
async fn can_read_from_existing_buf() {
    let mut parts = FramedParts::new(DontReadIntoThis, U32Codec);
    parts.read_buf = vec![0, 0, 0, 42].into();

    let mut framed = Framed::from_parts(parts);
    let num = assert_ok!(framed.next().await.unwrap());

    assert_eq!(num, 42);
}

#[test]
fn external_buf_grows_to_init() {
    let mut parts = FramedParts::new(DontReadIntoThis, U32Codec);
    parts.read_buf = vec![0, 0, 0, 42].into();

    let framed = Framed::from_parts(parts);
    let FramedParts { read_buf, .. } = framed.into_parts();

    assert_eq!(read_buf.capacity(), INITIAL_CAPACITY);
}

#[test]
fn external_buf_does_not_shrink() {
    let mut parts = FramedParts::new(DontReadIntoThis, U32Codec);
    parts.read_buf = vec![0; INITIAL_CAPACITY * 2].into();

    let framed = Framed::from_parts(parts);
    let FramedParts { read_buf, .. } = framed.into_parts();

    assert_eq!(read_buf.capacity(), INITIAL_CAPACITY * 2);
}
