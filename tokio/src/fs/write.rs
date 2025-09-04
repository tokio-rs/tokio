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
    let contents = crate::util::as_ref::upgrade(contents);

    #[cfg(all(tokio_uring, feature = "rt", feature = "fs", target_os = "linux"))]
    {
        let handle = crate::runtime::Handle::current();
        let driver_handle = handle.inner.driver().io();
        if driver_handle.check_and_init()? {
            return write_uring(path, contents).await;
        }
    }

    write_spawn_blocking(path, contents).await
}

#[cfg(all(tokio_uring, feature = "rt", feature = "fs", target_os = "linux"))]
async fn write_uring(path: &Path, contents: OwnedBuf) -> io::Result<()> {
    use crate::{fs::OpenOptions, runtime::driver::op::Op};
    use std::os::fd::AsFd;

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;

    let fd = file.as_fd();

    let mut pos = 0;
    let mut buf = contents.as_ref();
    while !buf.is_empty() {
        let n = Op::write_at(fd, buf, pos)?.await? as usize;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        buf = &buf[n..];
        pos += n as u64;
    }

    Ok(())
}

async fn write_spawn_blocking(path: &Path, contents: OwnedBuf) -> io::Result<()> {
    let path = path.to_owned();
    asyncify(move || std::fs::write(path, contents)).await
}
