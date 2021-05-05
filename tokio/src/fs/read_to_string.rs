use crate::fs::asyncify;

use std::{io, path::Path};

/// Creates a future which will open a file for reading and read the entire
/// contents into a string and return said string.
///
/// This is the async equivalent of [`std::fs::read_to_string`][std].
///
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
    let path = path.as_ref().to_owned();
    asyncify(move || std::fs::read_to_string(path)).await
}
