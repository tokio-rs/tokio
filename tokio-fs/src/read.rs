use crate::File;

use tokio_io::AsyncReadExt;

use std::{io, path::Path};

/// Creates a future which will open a file for reading and read the entire
/// contents into a buffer and return said buffer.
///
/// This is the async equivalent of `std::fs::read`.
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// let contents = fs::read("foo.txt").await?;
/// println!("foo.txt contains {} bytes", contents.len());
/// # Ok(())
/// # }
/// ```
pub async fn read<P>(path: P) -> io::Result<Vec<u8>>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    let mut file = File::open(path).await?;
    let metadata = file.metadata().await?;

    let mut contents = Vec::with_capacity(metadata.len() as usize + 1);
    file.read_to_end(&mut contents).await?;
    Ok(contents)
}
