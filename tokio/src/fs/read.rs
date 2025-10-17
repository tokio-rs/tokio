use crate::fs::asyncify;

use std::{io, path::Path};

/// Reads the entire contents of a file into a bytes vector.
///
/// This is an async version of [`std::fs::read`].
///
/// This is a convenience function for using [`File::open`] and [`read_to_end`]
/// with fewer imports and without an intermediate variable. It pre-allocates a
/// buffer based on the file size when available, so it is generally faster than
/// reading into a vector created with `Vec::new()`.
///
/// This operation is implemented by running the equivalent blocking operation
/// on a separate thread pool using [`spawn_blocking`].
///
/// [`File::open`]: super::File::open
/// [`read_to_end`]: crate::io::AsyncReadExt::read_to_end
/// [`spawn_blocking`]: crate::task::spawn_blocking
///
/// # Errors
///
/// This function will return an error if `path` does not already exist.
/// Other errors may also be returned according to [`OpenOptions::open`].
///
/// [`OpenOptions::open`]: super::OpenOptions::open
///
/// It will also return an error if it encounters while reading an error
/// of a kind other than [`ErrorKind::Interrupted`].
///
/// [`ErrorKind::Interrupted`]: std::io::ErrorKind::Interrupted
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
/// use std::net::SocketAddr;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
///     let contents = fs::read("address.txt").await?;
///     let foo: SocketAddr = String::from_utf8_lossy(&contents).parse()?;
///     Ok(())
/// }
/// ```
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref().to_owned();

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
        if driver_handle.check_and_init()? {
            return read_uring(&path).await;
        }
    }

    asyncify(move || std::fs::read(path)).await
}

#[cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]
async fn read_uring(path: &Path) -> io::Result<Vec<u8>> {
    use crate::{fs::OpenOptions, runtime::driver::op::Op};
    use std::os::fd::OwnedFd;

    let file = OpenOptions::new().read(true).open(path).await?;

    let size = file
        .metadata()
        .await
        .map(|m| m.len() as usize)
        .ok()
        .unwrap_or(0);

    let buf = Vec::with_capacity(size);

    let fd: OwnedFd = file
        .try_into_std()
        .expect("unexpected in-flight operation detected")
        .into();

    let (read_size, mut buf) = Op::read(fd, buf)?.await?;

    // SAFETY:
    // 1. The buffer is initialized with `size` capacity
    // 2. the read_size is the number of bytes read from the file
    unsafe { buf.set_len(read_size as _) };

    Ok(buf)
}
