use crate::asyncify;

use std::io;
use std::path::Path;

/// Removes a directory at this path, after removing all its contents. Use carefully!
///
/// This is an async version of [`std::fs::remove_dir_all`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_dir_all.html
pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    asyncify(|| std::fs::remove_dir_all(&path)).await
}
