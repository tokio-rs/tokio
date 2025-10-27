#[cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]
use crate::io::blocking;
use crate::{fs::asyncify, util::as_ref::OwnedBuf};

use std::{io, path::Path};

/// Creates a future that will open a file for writing and write the entire
/// contents of `contents` to it.
///
/// This is the async equivalent of [`std::fs::write`][std].
///
/// This operation is implemented by running the equivalent blocking operation
/// on a separate thread pool using [`spawn_blocking`].
///
/// [`spawn_blocking`]: crate::task::spawn_blocking
/// [std]: fn@std::fs::write
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// fs::write("foo.txt", b"Hello world!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();

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
            use crate::io::blocking;

            let mut buf = blocking::Buf::with_capacity(contents.as_ref().len());
            buf.copy_from(contents.as_ref(), contents.as_ref().len());
            return write_uring(path, buf).await;
        }
    }

    let contents = crate::util::as_ref::upgrade(contents);
    write_spawn_blocking(path, contents).await
}

#[cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]
async fn write_uring(path: &Path, mut buf: blocking::Buf) -> io::Result<()> {
    use crate::{fs::OpenOptions, io::uring::utils::ArcFd, runtime::driver::op::Op};
    use std::sync::Arc;

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;

    let mut fd: ArcFd = Arc::new(
        file.try_into_std()
            .expect("unexpected in-flight operation detected"),
    );

    let mut file_offset: u64 = 0;
    while !buf.is_empty() {
        let (n, _buf, _fd) = Op::write_at(fd, buf, file_offset).await;
        // TODO: handle EINT here
        let n = n?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }

        buf = _buf;
        fd = _fd;
        file_offset += n as u64;
    }

    Ok(())
}

async fn write_spawn_blocking(path: &Path, contents: OwnedBuf) -> io::Result<()> {
    let path = path.to_owned();
    asyncify(move || std::fs::write(path, contents)).await
}
