use crate::fs::asyncify;

use std::io;
use std::path::Path;

/// Removes a directory at this path, after removing all its contents. Use carefully!
///
/// This is an async version of [`std::fs::remove_dir_all`][std]
///
/// [std]: fn@std::fs::remove_dir_all
pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    assert!(std::env::var("XXX_KEEP_UNTESTED_XXX").is_ok());
    let path = path.as_ref().to_owned();
    asyncify(move || std::fs::remove_dir_all(path)).await
}
