//! Utilities for encoding and decoding frames.
//!
//! Contains adapters to go from streams of bytes, [`AsyncRead`] and
//! [`AsyncWrite`], to framed streams implementing [`Sink`] and [`Stream`].
//! Framed streams are also known as [transports].
//!
//! [`AsyncRead`]: #
//! [`AsyncWrite`]: #
//! [`Sink`]: #
//! [`Stream`]: #
//! [transports]: #

#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-codec/0.1.0")]

extern crate bytes;
extern crate tokio_io;

mod bytes_codec;
mod lines_codec;

pub use tokio_io::_tokio_codec::{
    Decoder,
    Encoder,
    Framed,
    FramedParts,
    FramedRead,
    FramedWrite,
};

pub use bytes_codec::BytesCodec;
pub use lines_codec::LinesCodec;
