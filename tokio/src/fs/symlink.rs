use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Creates a new symbolic link on the filesystem.
///
/// The `link` path will be a symbolic link pointing to the `original` path.
///
/// This is an async version of [`std::os::unix::fs::symlink`].
pub async fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let original = original.as_ref().to_owned();
    let link = link.as_ref().to_owned();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux"
    ))]
    {
        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle.check_and_init(io_uring::opcode::SymlinkAt::CODE)? {
            return crate::runtime::driver::op::Op::symlink(&original, &link)?.await;
        }
    }

    asyncify(move || std::os::unix::fs::symlink(original, link)).await
}
