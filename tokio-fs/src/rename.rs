use crate::asyncify;

use std::io;
use std::path::Path;

/// Rename a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is an async version of [`std::fs::rename`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.rename.html
pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    asyncify(|| std::fs::rename(&from, &to)).await
}
