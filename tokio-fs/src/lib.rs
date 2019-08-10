#![doc(html_root_url = "https://docs.rs/tokio-fs/0.2.0-alpha.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

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

mod create_dir;
mod create_dir_all;
mod file;
mod hard_link;
mod metadata;
mod open_options;
pub mod os;
mod read;
mod read_dir;
mod read_link;
mod remove_dir;
mod remove_dir_all;
mod remove_file;
mod rename;
mod set_permissions;
mod stderr;
mod stdin;
mod stdout;
mod symlink_metadata;
mod write;

pub use crate::create_dir::create_dir;
pub use crate::create_dir_all::create_dir_all;
pub use crate::file::File;
pub use crate::hard_link::hard_link;
pub use crate::metadata::metadata;
pub use crate::open_options::OpenOptions;
pub use crate::read::read;
pub use crate::read_dir::{read_dir, DirEntry, ReadDir};
pub use crate::read_link::read_link;
pub use crate::remove_dir::remove_dir;
pub use crate::remove_dir_all::remove_dir_all;
pub use crate::remove_file::remove_file;
pub use crate::rename::rename;
pub use crate::set_permissions::set_permissions;
pub use crate::stderr::{stderr, Stderr};
pub use crate::stdin::{stdin, Stdin};
pub use crate::stdout::{stdout, Stdout};
pub use crate::symlink_metadata::symlink_metadata;
pub use crate::write::write;

use std::io;
use std::io::ErrorKind::Other;
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

async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    use futures_util::future::poll_fn;

    let mut f = Some(f);
    poll_fn(move |_| blocking_io(|| f.take().unwrap()())).await
}

fn blocking_err() -> io::Error {
    io::Error::new(
        Other,
        "`blocking` annotated I/O must be called \
         from the context of the Tokio runtime.",
    )
}
