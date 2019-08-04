//! A "prelude" for users of the `tokio` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually:
//!
//! ```
//! use tokio::prelude::*;
//! ```
//!
//! The prelude may grow over time as additional items see ubiquitous use.

pub use crate::future::FutureExt as _;
pub use futures_util::future::FutureExt as _;
pub use std::future::Future;

pub use crate::stream::{Stream, StreamExt as _};
pub use futures_util::stream::StreamExt as _;

#[cfg(feature = "io")]
pub use tokio_io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _,
};
