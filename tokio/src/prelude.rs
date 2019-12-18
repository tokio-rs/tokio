#![cfg(not(loom))]

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

pub use crate::io::{self, AsyncBufRead, AsyncRead, AsyncWrite};

cfg_io_util! {
    #[doc(no_inline)]
    pub use crate::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};
}
