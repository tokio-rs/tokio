//! Asynchronous filesystem manipulation operations.
//!
//! This module contains basic methods and types for manipulating the contents
//! of the local filesystem from within the context of the Tokio runtime.
//!
//! Unlike *most* other Tokio APIs, the filesystem APIs **must** be used from
//! the context of the Tokio runtime as they require Tokio specific features to
//! function.

pub use tokio_fs::OpenOptions;
pub use tokio_fs::{
    create_dir, create_dir_all, file, hard_link, metadata, os, read_dir, read_link,
};
pub use tokio_fs::{read, write, ReadFile, WriteFile};
pub use tokio_fs::{remove_dir, remove_file, rename, set_permissions, symlink_metadata, File};
