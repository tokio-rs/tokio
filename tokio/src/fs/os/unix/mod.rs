//! Unix-specific extensions to primitives in the `tokio_fs` module.

mod symlink;
pub use self::symlink::symlink;
