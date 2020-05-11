//! Unix-specific extensions to primitives in the `tokio_fs` module.

mod symlink;
pub use self::symlink::symlink;

mod open_options_ext;
pub use self::open_options_ext::OpenOptionsExt;
