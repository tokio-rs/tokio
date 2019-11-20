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

cfg_io_blocking! {
    pub(crate) mod blocking;
}

mod async_buf_read;
pub use self::async_buf_read::AsyncBufRead;

mod async_read;
pub use self::async_read::AsyncRead;

mod async_write;
pub use self::async_write::AsyncWrite;

cfg_io_driver! {
    pub(crate) mod driver;

    mod poll_evented;
    pub use poll_evented::PollEvented;

    mod registration;
    pub use registration::Registration;
}

cfg_io_std! {
    mod stderr;
    pub use stderr::{stderr, Stderr};

    mod stdin;
    pub use stdin::{stdin, Stdin};

    mod stdout;
    pub use stdout::{stdout, Stdout};
}

cfg_io_util! {
    mod split;
    pub use split::{split, ReadHalf, WriteHalf};

    pub(crate) mod util;
    pub use util::{
        copy, empty, repeat, sink, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufStream,
        BufWriter, Copy, Empty, Lines, Repeat, Sink, Split, Take,
    };

    // Re-export io::Error so that users don't have to deal with conflicts when
    // `use`ing `tokio::io` and `std::io`.
    pub use std::io::{Error, ErrorKind, Result};
}

cfg_not_io_util! {
    cfg_process! {
        pub(crate) mod util;
    }
}

cfg_io_blocking! {
    /// Types in this module can be mocked out in tests.
    mod sys {
        // TODO: don't rename
        pub(crate) use crate::blocking::spawn_blocking as run;
        pub(crate) use crate::task::JoinHandle as Blocking;
    }
}
