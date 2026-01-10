use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Removes a file from the filesystem.
///
/// Note that there is no guarantee that the file is immediately deleted (e.g.
/// depending on platform, other open file descriptors may prevent immediate
/// removal).
///
/// This is an async version of [`std::fs::remove_file`].
pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux",
    ))]
    {
        use crate::io::uring::unlink::Unlink;
        use crate::runtime::driver::op::Op;

        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        type OpUnlink = Op<Unlink>;

        if driver_handle.check_and_init(OpUnlink::CODE)? {
            return OpUnlink::remove_file(path)?.await;
        }
    }

    remove_file_blocking(path).await
}

async fn remove_file_blocking(path: &Path) -> io::Result<()> {
    let path = path.to_owned();
    asyncify(move || std::fs::remove_file(path)).await
}
