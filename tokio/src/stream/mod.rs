//! Stream utilities for Tokio.
//!
//! A `Stream` is an asynchronous sequence of values. It can be thought of as
//! an asynchronous version of the standard library's `Iterator` trait.
//!
//! This module provides helpers to work with them. For examples of usage and a more in-depth
//! description of streams you can also refer to the [streams
//! tutorial](https://tokio.rs/tokio/tutorial/streams) on the tokio website.
//!
//! # Iterating over a Stream
//!
//! Due to similarities with the standard library's `Iterator` trait, some new
//! users may assume that they can use `for in` syntax to iterate over a
//! `Stream`, but this is unfortunately not possible. Instead, you can use a
//! `while let` loop as follows:
//!
//! ```rust
//! use tokio::stream::{self, StreamExt};
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut stream = stream::iter(vec![0, 1, 2]);
//!
//!     while let Some(value) = stream.next().await {
//!         println!("Got {}", value);
//!     }
//! }
//! ```
//!
//! # Returning a Stream from a function
//!
//! A common way to stream values from a function is to pass in the sender
//! half of a channel and use the receiver as the stream. This requires awaiting
//! both futures to ensure progress is made. Another alternative is the
//! [async-stream] crate, which contains macros that provide a `yield` keyword
//! and allow you to return an `impl Stream`.
//!
//! [async-stream]: https://docs.rs/async-stream
//!
//! # Conversion to and from AsyncRead/AsyncWrite
//!
//! It is often desirable to convert a `Stream` into an [`AsyncRead`],
//! especially when dealing with plaintext formats streamed over the network.
//! The opposite conversion from an [`AsyncRead`] into a `Stream` is also
//! another commonly required feature. To enable these conversions,
//! [`tokio-util`] provides the [`StreamReader`] and [`ReaderStream`]
//! types when the io feature is enabled.
//!
//! [tokio-util]: https://docs.rs/tokio-util/0.3/tokio_util/codec/index.html
//! [`tokio::io`]: crate::io
//! [`AsyncRead`]: crate::io::AsyncRead
//! [`AsyncWrite`]: crate::io::AsyncWrite
//! [`ReaderStream`]: https://docs.rs/tokio-util/0.4/tokio_util/io/struct.ReaderStream.html
//! [`StreamReader`]: https://docs.rs/tokio-util/0.4/tokio_util/io/struct.StreamReader.html

pub use tokio_stream::FromStream;
pub use tokio_stream::StreamExt;
pub use tokio_stream::StreamMap;
pub use tokio_stream::{empty, Empty};
pub use tokio_stream::{iter, Iter};
pub use tokio_stream::{once, Once};
pub use tokio_stream::{pending, Pending};

#[doc(no_inline)]
pub use futures_core::Stream;
