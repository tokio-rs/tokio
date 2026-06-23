use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Returns `Ok(true)` if the path points at an existing entity.
///
/// This function will traverse symbolic links to query information about the
/// destination file. In case of broken symbolic links this will return `Ok(false)`.
///
/// This is the async equivalent of [`std::path::Path::try_exists`][std].
///
/// [std]: fn@std::path::Path::try_exists
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// fs::try_exists("foo.txt").await?;
/// # Ok(())
/// # }
/// ```
pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
    let path = path.as_ref();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        // libc::statx is only supported on these platforms
        // FIXME: Add musl target env when our minimum supported
        // rust version is 1.93. To clarify, statx support is
        // introduced to musl in 1.25 as mentioned officially here:
        // https://musl.libc.org/releases.html.
        // However, rustup target_env building for *-linux-musl
        // uses 1.25 musl on all *-linux-musl platforms starting
        // in 1.93 stable rust version.
        // https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5/
        any(target_env = "gnu", target_os = "android")
    ))]
    {
        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle
            .check_and_init(io_uring::opcode::Statx::CODE)
            .await?
        {
            return try_exists_uring(path).await;
        }
    }

    try_exists_spawn_blocking(path).await
}

cfg_io_uring! {
    #[inline]
    #[cfg(
        // libc::statx is only supported on these platforms
        // FIXME: Add musl target env when our minimum supported
        // rust version is 1.93. To clarify, statx support is
        // introduced to musl in 1.25 as mentioned officially here:
        // https://musl.libc.org/releases.html.
        // However, rustup target_env building for *-linux-musl
        // uses 1.25 musl on all *-linux-musl platforms starting
        // in 1.93 stable rust version.
        // https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5/
        any(target_env = "gnu", target_os = "android")
    )]
    async fn try_exists_uring(path: &Path) -> io::Result<bool> {
        use crate::runtime::driver::op::Op;

        match Op::metadata(path)?.await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
}

async fn try_exists_spawn_blocking(path: &Path) -> io::Result<bool> {
    let path = path.to_owned();
    // FIXME: When MSRV is 1.81, change this to
    // std::fs::exists() to be consistent with
    // all other tokio::fs operations
    asyncify(move || path.try_exists()).await
}
