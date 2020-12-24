//! `UnixStream` owned split support.
//!
//! A `UnixStream` can be split into an `OwnedReadHalf` and a `OwnedWriteHalf`
//! with the `UnixStream::into_split` method.  `OwnedReadHalf` implements
//! `AsyncRead` while `OwnedWriteHalf` implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.

pub use t10::net::unix::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};
