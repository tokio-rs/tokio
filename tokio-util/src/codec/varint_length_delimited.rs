//! Frame a stream of bytes with a protobuf style varint length prefix
//!
//! Protocols built on protobuf (among others) delimit messages by prefacing the
//! payload with its length encoded as a base 128 varint: seven bits of the
//! length per byte, with the high bit set while more bytes follow. This module
//! provides a codec for that framing, so a byte stream can be turned into a
//! stream of frames without implementing the parsing by hand.
//!
//! [`VarintLengthDelimitedCodec::new`] returns a codec with the default
//! configuration, which refuses frames larger than 8 MiB:
//!
//! ```
//! use bytes::Bytes;
//! use futures::SinkExt;
//! use tokio_util::codec::{FramedWrite, VarintLengthDelimitedCodec};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let mut writer = FramedWrite::new(Vec::new(), VarintLengthDelimitedCodec::new());
//! writer.send(Bytes::from_static(b"hello")).await.unwrap();
//!
//! // The payload is prefixed with its length, "hello" being five bytes long.
//! assert_eq!(writer.get_ref().as_slice(), b"\x05hello");
//! # }
//! ```
//!
//! Decoding works the same way, with lengths using as many bytes as the payload
//! needs:
//!
//! ```
//! use tokio_stream::StreamExt;
//! use tokio_util::codec::{FramedRead, VarintLengthDelimitedCodec};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let mut reader = FramedRead::new(
//!     &b"\x05hello\x05world"[..],
//!     VarintLengthDelimitedCodec::new(),
//! );
//!
//! assert_eq!(reader.next().await.unwrap().unwrap().as_ref(), b"hello");
//! assert_eq!(reader.next().await.unwrap().unwrap().as_ref(), b"world");
//! assert!(reader.next().await.is_none());
//! # }
//! ```
//!
//! A length prefix that does not decode to a `u64` within ten bytes, or that
//! announces a frame larger than the configured maximum, is reported as an
//! [`io::Error`] of kind [`InvalidData`]. Encoded frames larger than the
//! maximum are rejected with [`InvalidInput`].
//!
//! [`InvalidData`]: std::io::ErrorKind::InvalidData
//! [`InvalidInput`]: std::io::ErrorKind::InvalidInput

use crate::codec::{Decoder, Encoder};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::{fmt, io};

/// The maximum number of bytes a protobuf style varint can occupy.
const MAX_VARINT_LEN: usize = 10;

/// The default maximum frame length, in bytes.
const DEFAULT_MAX_FRAME_LENGTH: usize = 8 * 1024 * 1024;

/// An error returned by [`VarintLengthDelimitedCodec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarintLengthDelimitedCodecError {
    /// The length prefix was not a valid varint encoding of a `u64`.
    InvalidVarint,
    /// The announced frame length exceeds the configured maximum.
    FrameTooLong {
        /// The length announced by the frame's prefix.
        len: u64,
        /// The maximum frame length the codec accepts.
        max: usize,
    },
}

impl fmt::Display for VarintLengthDelimitedCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVarint => f.write_str("invalid varint length prefix"),
            Self::FrameTooLong { len, max } => {
                write!(f, "frame of {len} bytes exceeds the maximum of {max} bytes")
            }
        }
    }
}

impl std::error::Error for VarintLengthDelimitedCodecError {}

/// A codec for frames delimited by a protobuf style varint length prefix.
///
/// The payload length is encoded using seven bits per byte, the high bit of a
/// byte being set while further bytes follow, as described in the [protobuf
/// encoding guide]. Lengths therefore take one byte up to 127, two bytes up to
/// 16383, and so on.
///
/// See the [module level] documentation for more detail.
///
/// [protobuf encoding guide]: https://protobuf.dev/programming-guides/encoding/#varints
/// [module level]: crate::codec
#[derive(Debug, Clone, Copy)]
pub struct VarintLengthDelimitedCodec {
    /// The maximum length of a frame payload, in bytes.
    max_frame_length: usize,
}

impl VarintLengthDelimitedCodec {
    /// Returns a `VarintLengthDelimitedCodec` with the default configuration.
    ///
    /// The returned codec refuses frames larger than 8 MiB. See
    /// [`new_with_max_frame_length`] for a discussion of why an upper bound
    /// matters.
    ///
    /// [`new_with_max_frame_length`]: Self::new_with_max_frame_length
    pub fn new() -> Self {
        Self::new_with_max_frame_length(DEFAULT_MAX_FRAME_LENGTH)
    }

    /// Returns a `VarintLengthDelimitedCodec` that refuses frames larger than
    /// `max_frame_length` bytes.
    ///
    /// A varint length prefix is read from the network before the payload, so a
    /// peer can announce a length far larger than the payload it ever sends. The
    /// maximum bounds how large a frame the codec will buffer for such a peer.
    pub fn new_with_max_frame_length(max_frame_length: usize) -> Self {
        Self { max_frame_length }
    }

    /// Sets the maximum length of a frame payload, in bytes.
    pub fn max_frame_length(&mut self, max_frame_length: usize) -> &mut Self {
        self.max_frame_length = max_frame_length;
        self
    }
}

impl Default for VarintLengthDelimitedCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for VarintLengthDelimitedCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        let (len, prefix_len) = match decode_length(src)? {
            Some(decoded) => decoded,
            None => return Ok(None),
        };

        if len > self.max_frame_length as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                VarintLengthDelimitedCodecError::FrameTooLong {
                    len,
                    max: self.max_frame_length,
                },
            ));
        }

        let len = len as usize;

        // The payload is buffered until the whole frame has arrived, so a
        // partial frame is neither consumed nor returned. The caller keeps
        // feeding the buffer; `FramedRead` grows it as needed.
        if src.len() - prefix_len < len {
            return Ok(None);
        }

        src.advance(prefix_len);

        Ok(Some(src.split_to(len)))
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        match self.decode(src)? {
            Some(frame) => Ok(Some(frame)),
            None if src.is_empty() => Ok(None),
            None => Err(io::Error::other("bytes remaining on stream")),
        }
    }
}

impl Encoder<&[u8]> for VarintLengthDelimitedCodec {
    type Error = io::Error;

    fn encode(&mut self, data: &[u8], dst: &mut BytesMut) -> io::Result<()> {
        if data.len() > self.max_frame_length {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                VarintLengthDelimitedCodecError::FrameTooLong {
                    len: data.len() as u64,
                    max: self.max_frame_length,
                },
            ));
        }

        encode_length(data.len() as u64, dst);
        dst.extend_from_slice(data);

        Ok(())
    }
}

impl Encoder<Bytes> for VarintLengthDelimitedCodec {
    type Error = io::Error;

    fn encode(&mut self, data: Bytes, dst: &mut BytesMut) -> io::Result<()> {
        Encoder::<&[u8]>::encode(self, data.as_ref(), dst)
    }
}

impl Encoder<BytesMut> for VarintLengthDelimitedCodec {
    type Error = io::Error;

    fn encode(&mut self, data: BytesMut, dst: &mut BytesMut) -> io::Result<()> {
        Encoder::<&[u8]>::encode(self, data.as_ref(), dst)
    }
}

/// Decodes a protobuf style varint length prefix.
///
/// Returns the length and the number of bytes the prefix occupied, or `None`
/// when the buffer does not hold a complete prefix yet. Bytes are never
/// consumed here: a partial prefix is left in place for the next call.
fn decode_length(src: &[u8]) -> io::Result<Option<(u64, usize)>> {
    let mut len = 0u64;

    for (idx, &byte) in src.iter().enumerate() {
        if idx + 1 == MAX_VARINT_LEN {
            // Only the lowest bit of a `u64` is left for the tenth byte, and it
            // has to be the last one.
            if byte & 0x80 != 0 || byte > 1 {
                return Err(invalid_varint());
            }
        }

        len |= u64::from(byte & 0x7f) << (7 * idx);

        if byte & 0x80 == 0 {
            return Ok(Some((len, idx + 1)));
        }
    }

    Ok(None)
}

/// Appends `len` to `dst` as a protobuf style varint.
fn encode_length(mut len: u64, dst: &mut BytesMut) {
    dst.reserve(varint_len(len));

    while len >= 0x80 {
        dst.put_u8((len as u8) | 0x80);
        len >>= 7;
    }

    dst.put_u8(len as u8);
}

/// Returns the number of bytes `len` occupies once encoded as a varint.
fn varint_len(mut len: u64) -> usize {
    let mut bytes = 1;

    while len >= 0x80 {
        len >>= 7;
        bytes += 1;
    }

    bytes
}

fn invalid_varint() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        VarintLengthDelimitedCodecError::InvalidVarint,
    )
}
