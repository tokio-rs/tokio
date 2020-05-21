//! Adaptors from AsyncRead/AsyncWrite to Stream/Sink
//!
//! Raw I/O objects work with byte sequences, but higher-level code
//! usually wants to batch these into meaningful chunks, called
//! "frames".
//!
//! This module contains adapters to go from streams of bytes,
//! [`AsyncRead`] and [`AsyncWrite`], to framed streams implementing
//! [`Sink`] and [`Stream`].  Framed streams are also known as
//! transports.
//!
//! [`AsyncRead`]: tokio::io::AsyncRead
//! [`AsyncWrite`]: tokio::io::AsyncWrite
//! [`Stream`]: tokio::stream::Stream
//! [`Sink`]: futures_sink::Sink

mod bytes_codec;
pub use self::bytes_codec::BytesCodec;

mod decoder;
pub use self::decoder::Decoder;

mod encoder;
pub use self::encoder::Encoder;

mod framed_impl;
#[allow(unused_imports)]
pub(crate) use self::framed_impl::{FramedImpl, RWFrames, ReadFrame, WriteFrame};

mod framed;
pub use self::framed::{Framed, FramedParts};

mod framed_read;
pub use self::framed_read::FramedRead;

mod framed_write;
pub use self::framed_write::FramedWrite;

pub mod length_delimited;
pub use self::length_delimited::{LengthDelimitedCodec, LengthDelimitedCodecError};

mod lines_codec;
pub use self::lines_codec::{LinesCodec, LinesCodecError};
