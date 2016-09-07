//! Asynchronous signal handling for Tokio
//!
//! This crate implements asynchronous signal handling for Tokio, and
//! asynchronous I/O framework in Rust. The primary type exported from this
//! crate, `unix::Signal`, allows listening for arbitrary signals on Unix
//! platforms, receiving them in an asynchronous fashion.
//!
//! Note that signal handling is in general a very tricky topic and should be
//! used with great care. This crate attempts to implement 'best practice' for
//! signal handling, but it should be evaluated for your own applications' needs
//! to see if it's suitable.
//!
//! The are some fundamental limitations of this crate documented on the
//! `Signal` structure as well.
//!
//! > **Note**: This crate compiles on Windows, but currently contains no
//! >           bindings. Windows does not have signals like Unix does, but it
//! >           does have a way to receive ctrl-c notifications at the console.
//! >           It's planned that this will be bound and exported outside the
//! >           `unix` module in the future!

#![deny(missing_docs)]

#[macro_use]
extern crate futures;
extern crate tokio_core;

pub mod unix;
