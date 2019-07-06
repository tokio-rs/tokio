#![doc(html_root_url = "https://docs.rs/tokio-fs/0.1.6")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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

#[macro_use]
extern crate tokio_futures;

mod create_dir;
mod create_dir_all;
pub mod file;
mod hard_link;
mod metadata;
pub mod os;
mod read;
mod read_dir;
mod read_link;
mod remove_dir;
mod remove_file;
mod rename;
mod set_permissions;
mod stderr;
mod stdin;
mod stdout;
mod symlink_metadata;
mod write;

pub use crate::create_dir::{create_dir, CreateDirFuture};
pub use crate::create_dir_all::{create_dir_all, CreateDirAllFuture};
pub use crate::file::File;
pub use crate::file::OpenOptions;
pub use crate::hard_link::{hard_link, HardLinkFuture};
pub use crate::metadata::{metadata, MetadataFuture};
pub use crate::read::{read, ReadFile};
pub use crate::read_dir::{read_dir, DirEntry, ReadDir, ReadDirFuture};
pub use crate::read_link::{read_link, ReadLinkFuture};
pub use crate::remove_dir::{remove_dir, RemoveDirFuture};
pub use crate::remove_file::{remove_file, RemoveFileFuture};
pub use crate::rename::{rename, RenameFuture};
pub use crate::set_permissions::{set_permissions, SetPermissionsFuture};
pub use crate::stderr::{stderr, Stderr};
pub use crate::stdin::{stdin, Stdin};
pub use crate::stdout::{stdout, Stdout};
pub use crate::symlink_metadata::{symlink_metadata, SymlinkMetadataFuture};
pub use crate::write::{write, WriteFile};

use std::io;
use std::io::ErrorKind::{Other, WouldBlock};
use std::task::Poll;
use std::task::Poll::*;

fn blocking_io<F, T>(f: F) -> Poll<io::Result<T>>
where
    F: FnOnce() -> io::Result<T>,
{
    match tokio_threadpool::blocking(f) {
        Ready(Ok(v)) => Ready(v),
        Ready(Err(_)) => Ready(Err(blocking_err())),
        Pending => Pending,
    }
}

fn would_block<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    match tokio_threadpool::blocking(f) {
        Ready(Ok(Ok(v))) => Ok(v),
        Ready(Ok(Err(err))) => {
            debug_assert_ne!(err.kind(), WouldBlock);
            Err(err)
        }
        Ready(Err(_)) => Err(blocking_err()),
        Pending => Err(blocking_err()),
    }
}

fn blocking_err() -> io::Error {
    io::Error::new(
        Other,
        "`blocking` annotated I/O must be called \
         from the context of the Tokio runtime.",
    )
}
