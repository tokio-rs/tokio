//! Asynchronous filesystem manipulation operations (and stdin, stdout, stderr).
//!
//! This module contains basic methods and types for manipulating the contents
//! of the local filesystem from within the context of the Tokio runtime.
//!
//! Tasks running on the Tokio runtime are expected to be asynchronous, i.e.,
//! they will not block the thread of execution. Filesystem operations do not
//! satisfy this requirement. In order to perform filesystem operations
//! asynchronously, this library uses the [`blocking`][blocking] annotation
//! to signal to the runtime that a blocking operation is being performed. This
//! allows the runtime to compensate.
//!
//! [blocking]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool/fn.blocking.html

#[macro_use]
extern crate futures;
extern crate tokio_io;
extern crate tokio_threadpool;

pub mod file;
mod stdin;
mod stdout;
mod stderr;

pub use file::File;
pub use stdin::{stdin, Stdin};
pub use stdout::{stdout, Stdout};
pub use stderr::{stderr, Stderr};

use futures::Poll;
use futures::Async::*;

use std::io;
use std::io::ErrorKind::{Other, WouldBlock};

fn blocking_io<F, T>(f: F) -> Poll<T, io::Error>
where F: FnOnce() -> io::Result<T>,
{
    match tokio_threadpool::blocking(f) {
        Ok(Ready(Ok(v))) => Ok(v.into()),
        Ok(Ready(Err(err))) => Err(err),
        Ok(NotReady) => Ok(NotReady),
        Err(_) => Err(blocking_err()),
    }
}

fn would_block<F, T>(f: F) -> io::Result<T>
where F: FnOnce() -> io::Result<T>,
{
    match tokio_threadpool::blocking(f) {
        Ok(Ready(Ok(v))) => Ok(v),
        Ok(Ready(Err(err))) => {
            debug_assert_ne!(err.kind(), WouldBlock);
            Err(err)
        }
        Ok(NotReady) => Err(WouldBlock.into()),
        Err(_) => Err(blocking_err()),
    }
}

fn blocking_err() -> io::Error {
    io::Error::new(Other, "`blocking` annotated I/O must be called \
                   from the context of the Tokio runtime.")
}
