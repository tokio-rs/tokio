use crate::asyncify;

use std::io;
use std::path::Path;

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
pub async fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    asyncify(|| std::fs::create_dir(&path)).await
}
