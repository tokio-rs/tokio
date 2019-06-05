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

pub use std::future::Future;
pub use std::task::{self, Poll};
//pub use crate::util::{FutureExt, StreamExt};
#[cfg(feature = "async-await-preview")]
#[doc(inline)]
pub use tokio_futures::{
    io::{AsyncReadExt, AsyncWriteExt},
    sink::SinkExt,
    stream::StreamExt as StreamAsyncExt,
};
#[cfg(feature = "io")]
pub use tokio_io::{AsyncRead, AsyncWrite};
