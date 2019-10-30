//! A "prelude" for users of the `tokio` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually:
//!
//! ```
//! # #![allow(warnings)]
//! use tokio::prelude::*;
//! ```
//!
//! The prelude may grow over time as additional items see ubiquitous use.

#[doc(no_inline)]
pub use crate::future::FutureExt as _;
#[doc(no_inline)]
pub use futures_util::future::FutureExt as _;
pub use std::future::Future;

pub use crate::stream::Stream;
#[doc(no_inline)]
pub use crate::stream::StreamExt as _;
pub use futures_sink::Sink;
#[doc(no_inline)]
pub use futures_util::sink::SinkExt as _;
#[doc(no_inline)]
pub use futures_util::stream::StreamExt as _;

#[cfg(feature = "io")]
pub use crate::io::{AsyncBufRead, AsyncRead, AsyncWrite};
#[cfg(feature = "io")]
#[doc(no_inline)]
pub use crate::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};
