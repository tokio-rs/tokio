use crate::platform::linux::uring::driver::Op;

use std::io;
use std::path::Path;

/// Removes an empty directory.
///
/// # Examples
///
/// ```no_run
/// use tokio::platform::linux::uring::fs::remove_dir;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     remove_dir("/some/dir").await?;
///     Ok(())
/// }
/// ```
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let op = Op::unlink_dir(path.as_ref())?;
    let completion = op.await;
    completion.result?;

    Ok(())
}
