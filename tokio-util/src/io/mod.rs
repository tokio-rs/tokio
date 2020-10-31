//! Helpers for IO related tasks.
//!
//! These types are often used in combination with hyper or reqwest, as they
//! allow converting between a hyper [`Body`] and [`AsyncRead`].
//!
//! [`Body`]: https://docs.rs/hyper/0.13/hyper/struct.Body.html
//! [`AsyncRead`]: tokio::io::AsyncRead

mod read_buf;
mod reader_stream;
mod stream_reader;

pub use self::read_buf::read_buf;
pub use self::reader_stream::ReaderStream;
pub use self::stream_reader::StreamReader;
pub use crate::util::poll_read_buf;
