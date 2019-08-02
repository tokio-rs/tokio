use crate::asyncify;

use std::io;
use std::path::Path;

/// Recursively create a directory and all of its parent components if they
/// are missing.
///
/// This is an async version of [`std::fs::create_dir_all`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir_all.html
pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    asyncify(|| std::fs::create_dir_all(&path)).await
}
