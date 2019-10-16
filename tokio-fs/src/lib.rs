#![doc(html_root_url = "https://docs.rs/tokio-fs/0.2.0-alpha.6")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

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
//! to a *backup* thread immediately. See [tokio-executor] for more details
//! of the threading model and [`blocking`].
//!
//! [`blocking`]: https://docs.rs/tokio-executor/0.2.0-alpha.2/tokio_executor/threadpool/fn.blocking.html
//! [`AsyncRead`]: https://docs.rs/tokio-io/0.1/tokio_io/trait.AsyncRead.html
//! [tokio-executor]: https://docs.rs/tokio-executor/0.2.0-alpha.2/tokio_executor/threadpool/index.html

mod blocking;
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
mod read_to_string;
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
pub use crate::read_to_string::read_to_string;
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

async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T> + Send + 'static,
    T: Send + 'static,
{
    sys::run(f).await
}

/// Types in this module can be mocked out in tests.
mod sys {
    pub(crate) use std::fs::File;

    pub(crate) use tokio_executor::blocking::{run, Blocking};
}
