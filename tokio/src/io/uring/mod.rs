//! Operations submitted directly to the runtime's io_uring instance.
//!
//! [`Ring`] submits several operations at once, either as a batch of
//! independent operations or as a chain of operations linked in order.
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

pub use ring::{Chain, Fsync, Nop, Read, Ring, RingFd, RingOp, Submit, Write};
