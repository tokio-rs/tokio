use crate::File;

use tokio_io::AsyncWriteExt;

use std::{io, path::Path};

/// Creates a future that will open a file for writing and write the entire
/// contents of `contents` to it.
///
/// This is the async equivalent of `std::fs::write`.
///
/// # Examples
///
/// ```no_run
/// #![feature(async_await)]
///
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// fs::write("foo.txt", b"Hello world!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn write<P, C: AsRef<[u8]> + Unpin>(path: P, contents: C) -> io::Result<()>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    let mut file = File::create(path).await?;
    file.write_all(contents.as_ref()).await?;

    Ok(())
}
