use bytes::{Bytes, BufMut, BytesMut};
use codec::{Encoder, Decoder};
use std::io;

/// A simple `Codec` implementation that just ships bytes around.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BytesCodec(());

impl BytesCodec {
    /// Creates a new `BytesCodec` for shipping around raw bytes.
    pub fn new() -> BytesCodec { BytesCodec(())  }
}

impl Decoder for BytesCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<BytesMut>, io::Error> {
        if buf.len() > 0 {
            let len = buf.len();
            Ok(Some(buf.split_to(len)))
        } else {
            Ok(None)
        }
    }
}

impl Encoder for BytesCodec {
    type Item = Bytes;
    type Error = io::Error;

    fn encode(&mut self, data: Bytes, buf: &mut BytesMut) -> Result<(), io::Error> {
        buf.reserve(data.len());
        buf.put(data);
        Ok(())
    }
}
