#![doc(html_root_url = "https://docs.rs/tokio-io/0.1.12")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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

pub use self::async_buf_read::AsyncBufRead;
pub use self::async_read::AsyncRead;
pub use self::async_write::AsyncWrite;

// Re-export `Buf` and `BufMut` since they are part of the API
pub use bytes::{Buf, BufMut};
