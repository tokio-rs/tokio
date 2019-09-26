//! Utilities for encoding and decoding frames.
//!
//! Contains adapters to go from streams of bytes, [`AsyncRead`] and
//! [`AsyncWrite`], to framed streams implementing [`Sink`] and [`Stream`].
//! Framed streams are also known as [transports].
//!
//! [`AsyncRead`]: ../io/trait.AsyncRead.html
//! [`AsyncWrite`]: ../io/trait.AsyncWrite.html
//! [`Sink`]: https://docs.rs/futures-sink-preview/*/futures_sink/trait.Sink.html
//! [`Stream`]: https://docs.rs/futures-core-preview/*/futures_core/stream/trait.Stream.html
//! [transports]: https://tokio.rs/docs/going-deeper/frames/

pub use tokio_codec::{
    length_delimited, BytesCodec, Decoder, Encoder, Framed, FramedParts, FramedRead, FramedWrite,
    LengthDelimitedCodec, LengthDelimitedCodecError, LinesCodec, LinesCodecError,
};
