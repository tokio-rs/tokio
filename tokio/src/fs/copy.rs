use crate::fs::File;
use crate::io;
use std::path::Path;

/// Copies the contents of one file to another. This function will also copy the permission bits of the original file to the destination file.
/// This function will overwrite the contents of to.
///
/// This is the async equivalent of `std::fs::copy`.
///
/// # Examples
///
/// ```no_run
/// use tokio::fs;
///
/// # async fn dox() -> std::io::Result<()> {
/// fs::copy("foo.txt", "bar.txt").await?;
/// # Ok(())
/// # }
/// ```

pub async fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<u64, std::io::Error> {
    let from = File::open(from).await?;
    let to = File::create(to).await?;
    let (mut from, mut to) = (io::BufReader::new(from), io::BufWriter::new(to));
    io::copy(&mut from, &mut to).await
}
