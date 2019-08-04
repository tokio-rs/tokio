use crate::asyncify;

use std::fs::Metadata;
use std::io;
use std::path::Path;

/// Queries the file system metadata for a path.
pub async fn metadata<P>(path: P) -> io::Result<Metadata>
where
    P: AsRef<Path> + Send + 'static,
{
    asyncify(|| std::fs::metadata(&path)).await
}
