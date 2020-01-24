#![cfg(not(loom))]

//! Asynchronous file and standard stream adaptation.
//!
//! This module contains utility methods and adapter types for input/output to
//! files or standard streams (`Stdin`, `Stdout`, `Stderr`), and
//! filesystem manipulation, for use within (and only within) a Tokio runtime.
//!
//! Tasks run by *worker* threads should not block, as this could delay
//! servicing reactor events. Portable filesystem operations are blocking,
//! however. This module offers adapters which use a `blocking` annotation
//! to inform the runtime that a blocking operation is required. When
//! necessary, this allows the runtime to convert the current thread from a
//! *worker* to a *backup* thread, where blocking is acceptable.
//!
//! ## Usage
//!
//! Where possible, users should prefer the provided asynchronous-specific
//! traits such as [`AsyncRead`], or methods returning a `Future` or `Poll`
//! type. Adaptions also extend to traits like `std::io::Read` where methods
//! return `std::io::Result`. Be warned that these adapted methods may return
//! `std::io::ErrorKind::WouldBlock` if a *worker* thread can not be converted
//! to a *backup* thread immediately.
//!
//! [`AsyncRead`]: https://docs.rs/tokio-io/0.1/tokio_io/trait.AsyncRead.html

mod canonicalize;
pub use self::canonicalize::canonicalize;

mod create_dir;
pub use self::create_dir::create_dir;

mod create_dir_all;
pub use self::create_dir_all::create_dir_all;

mod file;
pub use self::file::File;

mod hard_link;
pub use self::hard_link::hard_link;

mod metadata;
pub use self::metadata::metadata;

mod open_options;
pub use self::open_options::OpenOptions;

pub mod os;

mod read;
pub use self::read::read;

mod read_dir;
pub use self::read_dir::{read_dir, DirEntry, ReadDir};

mod read_link;
pub use self::read_link::read_link;

mod read_to_string;
pub use self::read_to_string::read_to_string;

mod remove_dir;
pub use self::remove_dir::remove_dir;

mod remove_dir_all;
pub use self::remove_dir_all::remove_dir_all;

mod remove_file;
pub use self::remove_file::remove_file;

mod rename;
pub use self::rename::rename;

mod set_permissions;
pub use self::set_permissions::set_permissions;

mod symlink_metadata;
pub use self::symlink_metadata::symlink_metadata;

mod write;
pub use self::write::write;

mod copy;
pub use self::copy::copy;

use std::io;

pub(crate) async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match sys::run(f).await {
        Ok(res) => res,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::Other,
            "background task failed",
        )),
    }
}

/// Types in this module can be mocked out in tests.
mod sys {
    pub(crate) use std::fs::File;

    // TODO: don't rename
    pub(crate) use crate::runtime::spawn_blocking as run;
    pub(crate) use crate::task::JoinHandle as Blocking;
}
