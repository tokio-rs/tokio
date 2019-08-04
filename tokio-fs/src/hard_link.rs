use crate::asyncify;

use std::io;
use std::path::Path;

/// Creates a new hard link on the filesystem.
///
/// The `dst` path will be a link pointing to the `src` path. Note that systems
/// often require these two paths to both be located on the same filesystem.
///
/// This is an async version of [`std::fs::hard_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.hard_link.html
pub async fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    asyncify(|| std::fs::hard_link(&src, &dst)).await
}
