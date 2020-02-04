#![deny(missing_docs, missing_debug_implementations)]
#![doc(html_root_url = "https://docs.rs/tokio-codec/0.1.2")]

//! Utilities for encoding and decoding frames.
//!
//! > **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved
//! into [`tokio_util::codec`] of the [`tokio-util` crate] behind the `codec`
//! feature flag.
//!
//! [`tokio_util::codec`]: https://docs.rs/tokio-util/latest/tokio_util/codec/index.html
//! [`tokio-util` crate]: https://docs.rs/tokio-util/latest/tokio_util
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

extern crate bytes;
extern crate tokio_io;

mod bytes_codec;
mod lines_codec;

pub use tokio_io::_tokio_codec::{Decoder, Encoder, Framed, FramedParts, FramedRead, FramedWrite};

pub use bytes_codec::BytesCodec;
pub use lines_codec::LinesCodec;
