use crate::asyncify;

use std::io;
use std::path::{Path, PathBuf};

/// Reads a symbolic link, returning the file that the link points to.
///
/// This is an async version of [`std::fs::read_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.read_link.html
pub async fn read_link<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
    asyncify(|| std::fs::read_link(&path)).await
}
