use crate::fs::asyncify;

use std::fs::Metadata;
use std::io;
use std::path::Path;

/// Queries the file system metadata for a path.
pub async fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path.as_ref().to_owned();
    asyncify(|| std::fs::metadata(path)).await
}
