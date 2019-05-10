//! Utilities for encoding and decoding frames.
//!
//! Contains adapters to go from streams of bytes, [`AsyncRead`] and
//! [`AsyncWrite`], to framed streams implementing [`Sink`] and [`Stream`].
//! Framed streams are also known as [transports].
//!
//! [`AsyncRead`]: ../io/trait.AsyncRead.html
//! [`AsyncWrite`]: ../io/trait.AsyncWrite.html
//! [`Sink`]: https://docs.rs/futures/0.1/futures/sink/trait.Sink.html
//! [`Stream`]: https://docs.rs/futures/0.1/futures/stream/trait.Stream.html
//! [transports]: https://tokio.rs/docs/going-deeper/frames/

pub mod length_delimited;

pub use self::length_delimited::LengthDelimitedCodec;
pub use tokio_codec::{
    BytesCodec, Decoder, Encoder, Framed, FramedParts, FramedRead, FramedWrite, LinesCodec,
};
