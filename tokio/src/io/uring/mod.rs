//! Operations submitted directly to the runtime's io_uring instance.
//!
//! [`Ring`] collects operations and submits them at once. They run
//! independently, unless they are linked to run in order.
//!
//! This module is only available on Linux, when Tokio is built with
//! `--cfg tokio_unstable` and the `io-uring` feature, and only works on runtimes
//! with io_uring enabled.

pub(crate) mod open;
pub(crate) mod read;
pub(crate) mod rename;
pub(crate) mod ring;
pub(crate) mod statx;
pub(crate) mod utils;
pub(crate) mod write;

pub use ring::{Ring, Submit};
