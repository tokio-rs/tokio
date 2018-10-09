#![doc(html_root_url = "https://docs.rs/tokio-buf/0.1.0")]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! Asynchronous stream of bytes.
//!
//! This crate contains the `BufStream` trait and a number of combinators for
//! this trait. The trait is similar to `Stream` in the `futures` library, but
//! instead of yielding arbitrary values, it only yields types that implement
//! `Buf` (i.e, byte collections).

extern crate bytes;
extern crate either;
#[macro_use]
extern crate futures;

pub mod buf_stream;

#[doc(inline)]
pub use buf_stream::BufStream;
