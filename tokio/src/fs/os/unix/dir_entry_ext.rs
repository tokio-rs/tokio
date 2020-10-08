use crate::fs::DirEntry;
use std::os::unix::fs::DirEntryExt as _;

/// Unix-specific extension methods for [`fs::DirEntry`].
///
/// This mirrors the definition of [`std::os::unix::fs::DirEntryExt`].
///
/// [`fs::DirEntry`]: crate::fs::DirEntry
/// [`std::os::unix::fs::DirEntryExt`]: std::os::unix::fs::DirEntryExt
pub trait DirEntryExt: sealed::Sealed {
    /// Returns the underlying `d_ino` field in the contained `dirent`
    /// structure.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::fs;
    /// use tokio::fs::os::unix::DirEntryExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut entries = fs::read_dir(".").await?;
    /// while let Some(entry) = entries.next_entry().await? {
    ///     // Here, `entry` is a `DirEntry`.
    ///     println!("{:?}: {}", entry.file_name(), entry.ino());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn ino(&self) -> u64;
}

impl DirEntryExt for DirEntry {
    fn ino(&self) -> u64 {
        self.as_inner().ino()
    }
}

impl sealed::Sealed for DirEntry {}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait Sealed {}
}
