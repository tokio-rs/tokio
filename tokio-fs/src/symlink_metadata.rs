use super::asyncify;

use std::fs::Metadata;
use std::io;
use std::path::Path;

/// Queries the file system metadata for a path.
///
/// This is an async version of [`std::fs::symlink_metadata`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.symlink_metadata.html
pub async fn symlink_metadata<P>(path: P) -> io::Result<Metadata>
where
    P: AsRef<Path> + Send + 'static,
{
    asyncify(|| std::fs::symlink_metadata(&path)).await
}
