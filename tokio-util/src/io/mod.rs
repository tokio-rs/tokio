//! Helpers for IO related tasks.
//!
//! These types are often used in combination with hyper or reqwest, as they
//! allow converting between a hyper [`Body`] and [`AsyncRead`].
//!
//! [`Body`]: https://docs.rs/hyper/0.13/hyper/struct.Body.html
//! [`AsyncRead`]: tokio::io::AsyncRead

mod reader_stream;
mod stream_reader;
mod poll_read_buf;

pub use self::poll_read_buf::poll_read_buf;
pub use self::reader_stream::ReaderStream;
pub use self::stream_reader::StreamReader;
