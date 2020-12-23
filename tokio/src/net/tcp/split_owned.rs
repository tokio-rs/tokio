//! `TcpStream` owned split support.
//!
//! A `TcpStream` can be split into an `OwnedReadHalf` and a `OwnedWriteHalf`
//! with the `TcpStream::into_split` method.  `OwnedReadHalf` implements
//! `AsyncRead` while `OwnedWriteHalf` implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.

#[doc(inline)]
pub use t10::net::tcp::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};
