use crate::fs::open_options::OpenOptions;
use std::os::unix::fs::OpenOptionsExt as _;

/// Unix-specific extensions to [`fs::OpenOptions`].
///
/// This mirrors the definition of [`std::os::unix::fs::OpenOptionsExt`].
///
/// [`fs::OpenOptions`]: crate::fs::OpenOptions
/// [`std::os::unix::fs::OpenOptionsExt`]: std::os::unix::fs::OpenOptionsExt
pub trait OpenOptionsExt: sealed::Sealed {
    /// Sets the mode bits that a new file will be created with.
    ///
    /// If a new file is created as part of an `OpenOptions::open` call then this
    /// specified `mode` will be used as the permission bits for the new file.
    /// If no `mode` is set, the default of `0o666` will be used.
    /// The operating system masks out bits with the system's `umask`, to produce
    /// the final permissions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::unix::OpenOptionsExt;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut options = OpenOptions::new();
    ///     options.mode(0o644); // Give read/write for owner and read for others.
    ///     let file = options.open("foo.txt").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    fn mode(&mut self, mode: u32) -> &mut Self;

    /// Pass custom flags to the `flags` argument of `open`.
    ///
    /// The bits that define the access mode are masked out with `O_ACCMODE`, to
    /// ensure they do not interfere with the access mode set by Rusts options.
    ///
    /// Custom flags can only set flags, not remove flags set by Rusts options.
    /// This options overwrites any previously set custom flags.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use libc;
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::unix::OpenOptionsExt;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut options = OpenOptions::new();
    ///     options.write(true);
    ///     if cfg!(unix) {
    ///         options.custom_flags(libc::O_NOFOLLOW);
    ///     }
    ///     let file = options.open("foo.txt").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    fn custom_flags(&mut self, flags: i32) -> &mut Self;
}

impl OpenOptionsExt for OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut OpenOptions {
        self.as_inner_mut().mode(mode);
        self
    }

    fn custom_flags(&mut self, flags: i32) -> &mut OpenOptions {
        self.as_inner_mut().custom_flags(flags);
        self
    }
}

impl sealed::Sealed for OpenOptions {}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait Sealed {}
}
