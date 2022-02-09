//! Helpers for IO related tasks.
//!
//! The stream types are often used in combination with hyper or reqwest, as they
//! allow converting between a hyper [`Body`] and [`AsyncRead`].
//!
//! The [`SyncIoBridge`] type converts from the world of async I/O
//! to synchronous I/O; this may often come up when using synchronous APIs
//! inside [`tokio::task::spawn_blocking`].
//!
//! [`Body`]: https://docs.rs/hyper/0.13/hyper/struct.Body.html
//! [`AsyncRead`]: tokio::io::AsyncRead

mod read_buf;
mod reader_stream;
mod stream_reader;
cfg_io_util! {
    mod sync_bridge;
    pub use self::sync_bridge::SyncIoBridge;
}

pub use self::read_buf::read_buf;
pub use self::reader_stream::ReaderStream;
pub use self::stream_reader::StreamReader;
pub use crate::util::{poll_read_buf, poll_write_buf};
