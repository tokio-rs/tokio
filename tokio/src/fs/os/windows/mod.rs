//! Windows-specific extensions for the primitives in the `tokio_fs` module.

mod symlink_dir;
pub use self::symlink_dir::symlink_dir;

mod symlink_file;
pub use self::symlink_file::symlink_file;
