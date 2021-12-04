use crate::fs::asyncify;
use std::path::Path;

/// Returns `Ok(true)` if the path points at an existing entity.
///
/// This function will traverse symbolic links to query information about the
/// destination file. In case of broken symbolic links this will return `Ok(false)`.
///
/// This is the async equivalent of [`std::fs::try_exists`][std].
///
/// [std]: fn@std::fs::try_exists
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

pub async fn try_exists(path: impl AsRef<Path>) -> Result<bool, std::io::Error> {
    let path = path.as_ref().to_owned();
    match asyncify(move || std::fs::metadata(path)).await {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
