use crate::asyncify;

use std::io;
use std::path::Path;

/// Removes an existing, empty directory.
///
/// This is an async version of [`std::fs::remove_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_dir.html
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    asyncify(|| std::fs::remove_dir(&path)).await
}
