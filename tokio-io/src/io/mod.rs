//! Asynchronous I/O.
//!
//! This module is the asynchronous version of `std::io`. Primarily, it
//! defines two traits, [`AsyncRead`] and [`AsyncWrite`], which extend the
//! `Read` and `Write` traits of the standard library.
//!
//! # AsyncRead and AsyncWrite
//!
//! [`AsyncRead`] and [`AsyncWrite`] must only be implemented for
//! non-blocking I/O types that integrate with the futures type system. In
//! other words, these types must never block the thread, and instead the
//! current task is notified when the I/O resource is ready.
//!
//! # Standard input and output
//!
//! Tokio provides asynchronous APIs to standard [input], [output], and [error].
//! These APIs are very similar to the ones provided by `std`, but they also
//! implement [`AsyncRead`] and [`AsyncWrite`].
//!
//! Unlike *most* other Tokio APIs, the standard input / output APIs
//! **must** be used from the context of the Tokio runtime as they require
//! Tokio specific features to function.
//!
//! [input]: fn.stdin.html
//! [output]: fn.stdout.html
//! [error]: fn.stderr.html
//!
//! # `std` re-exports
//!
//! Additionally, [`Error`], [`ErrorKind`], and [`Result`] are re-exported
//! from `std::io` for ease of use.
//!
//! [`AsyncRead`]: trait.AsyncRead.html
//! [`AsyncWrite`]: trait.AsyncWrite.html
//! [`Error`]: struct.Error.html
//! [`ErrorKind`]: enum.ErrorKind.html
//! [`Result`]: type.Result.html

mod async_buf_read_ext;
mod async_read_ext;
mod async_write_ext;
mod buf_reader;
mod buf_writer;
mod copy;
mod flush;
mod lines;
mod read;
mod read_exact;
mod read_line;
mod read_to_end;
mod read_to_string;
mod read_until;
mod shutdown;
mod write;
mod write_all;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_buf_read_ext::AsyncBufReadExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_read_ext::AsyncReadExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_write_ext::AsyncWriteExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_reader::BufReader;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_writer::BufWriter;

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/src/libstd/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: usize = 8 * 1024;
