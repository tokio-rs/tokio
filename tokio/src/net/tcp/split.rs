//! `TcpStream` split support.
//!
//! A `TcpStream` can be split into a `ReadHalf` and a
//! `WriteHalf` with the `TcpStream::split` method. `ReadHalf`
//! implements `AsyncRead` while `WriteHalf` implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.
pub use t10::net::tcp::{ReadHalf, ReuniteError, WriteHalf};
