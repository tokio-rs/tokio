#![doc(html_root_url = "https://docs.rs/tokio-codec/0.2.0-alpha.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Utilities for encoding and decoding frames.
//!
//! Contains adapters to go from streams of bytes, [`AsyncRead`] and
//! [`AsyncWrite`], to framed streams implementing [`Sink`] and [`Stream`].
//! Framed streams are also known as transports.
//!
//! [`AsyncRead`]: https://docs.rs/tokio/*/tokio/io/trait.AsyncRead.html
//! [`AsyncWrite`]: https://docs.rs/tokio/*/tokio/io/trait.AsyncWrite.html
//! [`Sink`]: https://docs.rs/futures-sink-preview/*/futures_sink/trait.Sink.html
//! [`Stream`]: https://docs.rs/futures-core-preview/*/futures_core/stream/trait.Stream.html

#[macro_use]
mod macros;

mod bytes_codec;
mod decoder;
mod encoder;
mod framed;
mod framed_read;
mod framed_write;
pub mod length_delimited;
mod lines_codec;

pub use crate::bytes_codec::BytesCodec;
pub use crate::decoder::Decoder;
pub use crate::encoder::Encoder;
pub use crate::framed::{Framed, FramedParts};
pub use crate::framed_read::FramedRead;
pub use crate::framed_write::FramedWrite;
pub use crate::length_delimited::{LengthDelimitedCodec, LengthDelimitedCodecError};
pub use crate::lines_codec::{LinesCodec, LinesCodecError};
