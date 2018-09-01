//! Asynchronous file and standard stream adaptation.
//!
//! This module contains utility methods and adapter types for input/output to
//! files or standard streams (`Stdin`, `Stdout`, `Stderr`), and
//! filesystem manipulation, for use within (and only within) a Tokio runtime.
//!
//! Tasks run by *worker* threads should not block, as this could delay
//! servicing reactor events. Portable filesystem operations are blocking,
//! however. This module offers adapters which use a [`blocking`] annotation
//! to inform the runtime that a blocking operation is required. When
//! necessary, this allows the runtime to convert the current thread from a
//! *worker* to a *backup* thread, where blocking is acceptable.
//!
//! ## Usage
//!
//! Where possible, users should prefer the provided asynchronous-specific
//! traits such as [`AsyncRead`], or methods returning a `Future` or `Poll`
//! type. Adaptions also extend to traits like `std::io::Read` where methods
//! return `std::io::Result`.  Be warned that these adapted methods may return
//! `std::io::ErrorKind::WouldBlock` if a *worker* thread can not be converted
//! to a *backup* thread immediately. See [tokio-threadpool] for more details
//! of the threading model and [`blocking`].
//!
//! [`blocking`]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool/fn.blocking.html
//! [`AsyncRead`]: https://docs.rs/tokio-io/0.1/tokio_io/trait.AsyncRead.html
//! [tokio-threadpool]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool

#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-fs/0.1.3")]

#[macro_use]
extern crate futures;
extern crate tokio_io;
extern crate tokio_threadpool;

mod create_dir;
mod create_dir_all;
pub mod file;
mod hard_link;
mod metadata;
pub mod os;
mod read_dir;
mod read_link;
mod remove_dir;
mod remove_file;
mod rename;
mod set_permissions;
mod stdin;
mod stdout;
mod stderr;
mod symlink_metadata;

pub use create_dir::{create_dir, CreateDirFuture};
pub use create_dir_all::{create_dir_all, CreateDirAllFuture};
pub use file::File;
pub use file::OpenOptions;
pub use hard_link::{hard_link, HardLinkFuture};
pub use metadata::{metadata, MetadataFuture};
pub use read_dir::{read_dir, ReadDirFuture, ReadDir, DirEntry};
pub use read_link::{read_link, ReadLinkFuture};
pub use remove_dir::{remove_dir, RemoveDirFuture};
pub use remove_file::{remove_file, RemoveFileFuture};
pub use rename::{rename, RenameFuture};
pub use set_permissions::{set_permissions, SetPermissionsFuture};
pub use stdin::{stdin, Stdin};
pub use stdout::{stdout, Stdout};
pub use stderr::{stderr, Stderr};
pub use symlink_metadata::{symlink_metadata, SymlinkMetadataFuture};

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
