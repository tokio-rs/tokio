use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Renames a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is an async version of [`std::fs::rename`].
pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux",
    ))]
    {
        use crate::io::uring::rename::Rename;
        use crate::runtime::driver::op::Op;

        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();

        type RenameOp = Op<Rename>;

        if driver_handle.check_and_init(RenameOp::CODE)? {
            return RenameOp::rename(from, to)?.await;
        }
    }

    rename_blocking(from, to).await
}

async fn rename_blocking(from: &Path, to: &Path) -> io::Result<()> {
    let [from, to] = [from, to].map(Path::to_owned);
    asyncify(move || std::fs::rename(from, to)).await
}
