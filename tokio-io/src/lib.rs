#![doc(html_root_url = "https://docs.rs/tokio-io/0.2.0-alpha.6")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Core I/O traits and combinators when working with Tokio.
//!
//! A description of the high-level I/O combinators can be [found online] in
//! addition to a description of the [low level details].
//!
//! [found online]: https://tokio.rs/docs/
//! [low level details]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/

mod async_buf_read;
mod async_read;
mod async_write;

#[cfg(feature = "util")]
mod io;

#[cfg(feature = "util")]
pub mod split;

pub use self::async_buf_read::AsyncBufRead;
pub use self::async_read::AsyncRead;
pub use self::async_write::AsyncWrite;

#[cfg(feature = "util")]
pub use self::io::{
    copy, empty, repeat, sink, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufStream,
    BufWriter, Copy, Empty, Repeat, Sink, Take,
};

// Re-export `Buf` and `BufMut` since they are part of the API
pub use bytes::{Buf, BufMut};

#[cfg(feature = "util")]
#[cfg(test)]
fn is_unpin<T: Unpin>() {}
