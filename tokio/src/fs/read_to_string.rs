use crate::fs::asyncify;

use std::{io, path::Path};

/// Creates a future which will open a file for reading and read the entire
/// contents into a string and return said string.
///
/// This is the async equivalent of [`std::fs::read_to_string`][std].
///
/// This operation is implemented by running the equivalent blocking operation
/// on a separate thread pool using [`spawn_blocking`].
///
/// [`spawn_blocking`]: crate::task::spawn_blocking
/// [std]: fn@std::fs::read_to_string
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// let contents = fs::read_to_string("foo.txt").await?;
/// println!("foo.txt contains {} bytes", contents.len());
/// # Ok(())
/// # }
/// ```
pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux"
    ))]
    {
        use crate::fs::read_uring;

        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle.check_and_init()? {
            return read_uring(path).await.and_then(|vec| {
                String::from_utf8(vec).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "stream did not contain valid UTF-8",
                    )
                })
            });
        }
    }

    read_to_string_spawn_blocking(path).await
}

async fn read_to_string_spawn_blocking(path: &Path) -> io::Result<String> {
    let path = path.to_owned();
    asyncify(move || std::fs::read_to_string(path)).await
}
