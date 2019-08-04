use crate::asyncify;

use std::io;
use std::path::Path;

/// Creates a new symbolic link on the filesystem.
///
/// The `dst` path will be a symbolic link pointing to the `src` path.
///
/// This is an async version of [`std::os::unix::fs::symlink`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/unix/fs/fn.symlink.html
pub async fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    asyncify(|| std::os::unix::fs::symlink(&src, &dst)).await
}
