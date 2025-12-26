use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Removes an existing, empty directory.
///
/// This is an async version of [`std::fs::remove_dir`].
pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux",
    ))]
    {
        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle.check_and_init()? {
            return remove_dir_uring(path).await;
        }
    }

    remove_dir_blocking(path).await
}

cfg_io_uring! {
    async fn remove_dir_uring(path: &Path) -> io::Result<()> {
        use crate::runtime::driver::op::Op;
        Op::remove_dir(path)?.await
    }
}

async fn remove_dir_blocking(path: &Path) -> io::Result<()> {
    let path = path.to_owned();
    asyncify(move || std::fs::remove_dir(path)).await
}
