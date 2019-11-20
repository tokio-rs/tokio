//! Asynchronous I/O.
//!
//! This module is the asynchronous version of `std::io`. Primarily, it
//! defines two traits, [`AsyncRead`] and [`AsyncWrite`], which are asynchronous
//! versions of the [`Read`] and [`Write`] traits in the standard library.
//!
//! # AsyncRead and AsyncWrite
//!
//! Like the standard library's [`Read`] and [`Write`] traits, [`AsyncRead`] and
//! [`AsyncWrite`] provide the most general interface for reading and writing
//! input and output. Unlike the standard library's traits, however, they are
//! _asynchronous_ &mdash; meaning that reading from or writing to a `tokio::io`
//! type will _yield_ to the Tokio scheduler when IO is not ready, rather than
//! blocking. This allows other tasks to run while waiting on IO.
//!
//! In asynchronous programs, Tokio's [`AsyncRead`] and [`AsyncWrite`] traits
//! can be used in almost exactly the same manner as the standard library's
//! `Read` and `Write`. Most types in the standard library that implement `Read`
//! and `Write` have asynchronous equivalents in `tokio` that implement
//! `AsyncRead` and `AsyncWrite`, such as [`File`] and [`TcpStream`].
//!
//! For example, the standard library documentation introduces `Read` by
//! [demonstrating][std_example] reading some bytes from a [`std::fs::File`]. We
//! can do the same with [`tokio::fs::File`][`File`]:
//!
//! ```no_run
//! use tokio::io;
//! use tokio::prelude::*;
//! use tokio::fs::File;
//!
//! #[tokio::main]
//! async fn main() -> io::Result<()> {
//!     let mut f = File::open("foo.txt").await?;
//!     let mut buffer = [0; 10];
//!
//!     // read up to 10 bytes
//!     let n = f.read(&mut buffer).await?;
//!
//!     println!("The bytes: {:?}", &buffer[..n]);
//!     Ok(())
//! }
//! ```
//!
//! [`File`]: crate::fs::File
//! [`TcpStream`]: crate::net::TcpStream
//! [`std::fs::File`]: std::fs::File
//! [std_example]: https://doc.rust-lang.org/std/io/index.html#read-and-write
//!
//! ## Buffered Readers and Writers
//!
//! Byte-based interfaces are unwieldy and can be inefficient, as we'd need to be
//! making near-constant calls to the operating system. To help with this,
//! `std::io` comes with [support for _buffered_ readers and writers][stdbuf],
//! and therefore, `tokio::io` does as well.
//!
//! Tokio provides an async version of the [`std::io::BufRead`] trait,
//! [`AsyncBufRead`]; and async [`BufReader`] and [`BufWriter`] structs, which
//! wrap readers and writers. These wrappers use a buffer, reducing the number
//! of calls and providing nicer methods for accessing exactly what you want.
//!
//! For example, [`BufReader`] works with the [`AsyncBufRead`] trait to add
//! extra methods to any async reader:
//!
//! ```no_run
//! use tokio::io;
//! use tokio::io::BufReader;
//! use tokio::fs::File;
//! use tokio::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> io::Result<()> {
//!     let f = File::open("foo.txt").await?;
//!     let mut reader = BufReader::new(f);
//!     let mut buffer = String::new();
//!
//!     // read a line into buffer
//!     reader.read_line(&mut buffer).await?;
//!
//!     println!("{}", buffer);
//!     Ok(())
//! }
//! ```
//!
//! [`BufWriter`] doesn't add any new ways of writing; it just buffers every call
//! to [`write`](crate::io::AsyncWriteExt::write):
//!
//! ```no_run
//! use tokio::io;
//! use tokio::io::BufReader;
//! use tokio::fs::File;
//! use tokio::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> io::Result<()> {
//!     let f = File::create("foo.txt").await?;
//!     {
//!         let mut writer = BufWriter::new(f);
//!
//!         // write a byte to the buffer
//!         writer.write(&[42]).await?;
//!
//!     } // the buffer is flushed once writer goes out of scope
//!
//!     Ok(())
//! }
//! ```
//!
//! [stdbuf]: https://doc.rust-lang.org/std/io/index.html#bufreader-and-bufwriter
//! [`std::io::BufRead`]: std::io::BufRead
//! [`AsyncBufRead`]: crate::io::AsyncBufRead
//! [`BufReader`]: crate::io::BufReader
//! [`BufWriter`]: crate::io::BufWriter
//!
//! ## Implementing AsyncRead and AsyncWrite
//!
//! Because they are traits, we can implement `AsyncRead` and `AsyncWrite` for
//! our own types, as well. Note that these traits must only be implemented for
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
//! Note that the standard input / output APIs  **must** be used from the
//! context of the Tokio runtime, as they require Tokio-specific features to function.
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
//! [`Read`]: std::io::Read
//! [`Write`]: std::io::Write
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
    pub mod split;
    pub use split::split;

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
