use std::path::Path;

/// Returns `true` if the path points at an existing entity.
///
/// This function will traverse symbolic links to query information about the
/// destination file.
///
/// # Examples
///
/// ```rust,no_run
/// # use tokio::fs;
///
/// # #[tokio::main]
/// # async fn main() {
/// let exists = fs::exists("/some/file/path.txt").await;
/// // in case the `/some/file/path.txt` points to existing path
/// assert_eq!(exists, true);
/// # }
/// ```
///
/// # Note
///
/// This method returns false in case of errors thus ignoring them. Use [`fs::metadata`][super::metadata()]
/// if you want to check for errors.
pub async fn exists(path: impl AsRef<Path>) -> bool {
    super::metadata(&path).await.is_ok()
}
