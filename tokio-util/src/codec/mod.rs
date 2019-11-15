//! Utilities for encoding and decoding frames.
//!
//! Contains adapters to go from streams of bytes, [`AsyncRead`] and
//! [`AsyncWrite`], to framed streams implementing [`Sink`] and [`Stream`].
//! Framed streams are also known as transports.
//!
//! [`AsyncRead`]: https://docs.rs/tokio/*/tokio/io/trait.AsyncRead.html
//! [`AsyncWrite`]: https://docs.rs/tokio/*/tokio/io/trait.AsyncWrite.html
//! [`Sink`]: https://docs.rs/futures-sink/*/futures_sink/trait.Sink.html
//! [`Stream`]: https://docs.rs/futures-core/*/futures_core/stream/trait.Stream.html

mod bytes_codec;
pub use self::bytes_codec::BytesCodec;

mod decoder;
pub use self::decoder::Decoder;

mod encoder;
pub use self::encoder::Encoder;

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
