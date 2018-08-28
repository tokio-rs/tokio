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
//! # Utility functions
//!
//! Utilities functions are provided for working with [`AsyncRead`] /
//! [`AsyncWrite`] types. For example, [`copy`] asynchronously copies all
//! data from a source to a destination.
//!
//! # `std` re-exports
//!
//! Additionally, [`Read`], [`Write`], [`Error`], [`ErrorKind`], and
//! [`Result`] are re-exported from `std::io` for ease of use.
//!
//! [`AsyncRead`]: trait.AsyncRead.html
//! [`AsyncWrite`]: trait.AsyncWrite.html
//! [`copy`]: fn.copy.html
//! [`Read`]: trait.Read.html
//! [`Write`]: trait.Write.html
//! [`Error`]: struct.Error.html
//! [`ErrorKind`]: enum.ErrorKind.html
//! [`Result`]: type.Result.html

mod copy;
mod flush;
mod read;
mod read_exact;
mod read_to_end;
mod read_until;
mod shutdown;
mod write_all;
mod lines;

pub use self::copy::{copy, Copy};
pub use self::flush::{flush, Flush};
pub use self::lines::{lines, Lines};
pub use self::read::{read, Read};
pub use self::read_exact::{read_exact, ReadExact};
pub use self::read_to_end::{read_to_end, ReadToEnd};
pub use self::read_until::{read_until, ReadUntil};
pub use self::shutdown::{shutdown, Shutdown};
pub use self::write_all::{write_all, WriteAll};

pub use tokio_io::{
    AsyncRead,
    AsyncWrite,
};

// standard input, output, and error
pub use tokio_fs::{
    stdin,
    Stdin,
    stdout,
    Stdout,
    stderr,
    Stderr,
};

// Re-export io::Error so that users don't have to deal
// with conflicts when `use`ing `futures::io` and `std::io`.
pub use ::std::io::{
    Error,
    ErrorKind,
    Result,
    // Read,
    // Write,
};
