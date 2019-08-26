use crate::asyncify;

use std::io;
use std::path::Path;

/// Removes a file from the filesystem.
///
/// Note that there is no
/// guarantee that the file is immediately deleted (e.g. depending on
/// platform, other open file descriptors may prevent immediate removal).
///
/// This is an async version of [`std::fs::remove_file`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_file.html
pub async fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    asyncify(move || std::fs::remove_file(path)).await
}
