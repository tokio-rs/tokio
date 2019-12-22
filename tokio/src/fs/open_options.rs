use crate::fs::{asyncify, File};

use std::io;
use std::path::Path;

/// Options and flags which can be used to configure how a file is opened.
///
/// This builder exposes the ability to configure how a [`File`] is opened and
/// what operations are permitted on the open file. The [`File::open`] and
/// [`File::create`] methods are aliases for commonly used options using this
/// builder.
///
/// Generally speaking, when using `OpenOptions`, you'll first call [`new`],
/// then chain calls to methods to set each option, then call [`open`], passing
/// the path of the file you're trying to open. This will give you a
/// [`io::Result`][result] with a [`File`] inside that you can further operate
/// on.
///
/// This is a specialized version of [`std::fs::OpenOptions`] for usage from
/// the Tokio runtime.
///
/// `From<std::fs::OpenOptions>` is implemented for more advanced configuration
/// than the methods provided here.
///
/// [`new`]: OpenOptions::new
/// [`open`]: OpenOptions::open
/// [result]: std::io::Result
/// [`File`]: File
/// [`File::open`]: File::open
/// [`File::create`]: File::create
/// [`std::fs::OpenOptions`]: std::fs::OpenOptions
///
/// # Examples
///
/// Opening a file to read:
///
/// ```no_run
/// use tokio::fs::OpenOptions;
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let file = OpenOptions::new()
///         .read(true)
///         .open("foo.txt")
///         .await?;
///
///     Ok(())
/// }
/// ```
///
/// Opening a file for both reading and writing, as well as creating it if it
/// doesn't exist:
///
/// ```no_run
/// use tokio::fs::OpenOptions;
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let file = OpenOptions::new()
///         .read(true)
///         .write(true)
///         .create(true)
///         .open("foo.txt")
///         .await?;
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug)]
pub struct OpenOptions(std::fs::OpenOptions);

impl OpenOptions {
    /// Creates a blank new set of options ready for configuration.
    ///
    /// All options are initially set to `false`.
    ///
    /// This is an async version of [`std::fs::OpenOptions::new`][std]
    ///
    /// [std]: std::fs::OpenOptions::new
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    ///
    /// let mut options = OpenOptions::new();
    /// let future = options.read(true).open("foo.txt");
    /// ```
    pub fn new() -> OpenOptions {
        OpenOptions(std::fs::OpenOptions::new())
    }

    /// Sets the option for read access.
    ///
    /// This option, when true, will indicate that the file should be
    /// `read`-able if opened.
    ///
    /// This is an async version of [`std::fs::OpenOptions::read`][std]
    ///
    /// [std]: std::fs::OpenOptions::read
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .read(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn read(&mut self, read: bool) -> &mut OpenOptions {
        self.0.read(read);
        self
    }

    /// Sets the option for write access.
    ///
    /// This option, when true, will indicate that the file should be
    /// `write`-able if opened.
    ///
    /// This is an async version of [`std::fs::OpenOptions::write`][std]
    ///
    /// [std]: std::fs::OpenOptions::write
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .write(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn write(&mut self, write: bool) -> &mut OpenOptions {
        self.0.write(write);
        self
    }

    /// Sets the option for the append mode.
    ///
    /// This option, when true, means that writes will append to a file instead
    /// of overwriting previous contents.  Note that setting
    /// `.write(true).append(true)` has the same effect as setting only
    /// `.append(true)`.
    ///
    /// For most filesystems, the operating system guarantees that all writes are
    /// atomic: no writes get mangled because another process writes at the same
    /// time.
    ///
    /// One maybe obvious note when using append-mode: make sure that all data
    /// that belongs together is written to the file in one operation. This
    /// can be done by concatenating strings before passing them to [`write()`],
    /// or using a buffered writer (with a buffer of adequate size),
    /// and calling [`flush()`] when the message is complete.
    ///
    /// If a file is opened with both read and append access, beware that after
    /// opening, and after every write, the position for reading may be set at the
    /// end of the file. So, before writing, save the current position (using
    /// [`seek`]`(`[`SeekFrom`]`::`[`Current`]`(0))`), and restore it before the next read.
    ///
    /// This is an async version of [`std::fs::OpenOptions::append`][std]
    ///
    /// [std]: std::fs::OpenOptions::append
    ///
    /// ## Note
    ///
    /// This function doesn't create the file if it doesn't exist. Use the [`create`]
    /// method to do so.
    ///
    /// [`write()`]: crate::io::AsyncWriteExt::write
    /// [`flush()`]: crate::io::AsyncWriteExt::flush
    /// [`seek`]: crate::io::AsyncSeekExt::seek
    /// [`SeekFrom`]: std::io::SeekFrom
    /// [`Current`]: std::io::SeekFrom::Current
    /// [`create`]: OpenOptions::create
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .append(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn append(&mut self, append: bool) -> &mut OpenOptions {
        self.0.append(append);
        self
    }

    /// Sets the option for truncating a previous file.
    ///
    /// If a file is successfully opened with this option set it will truncate
    /// the file to 0 length if it already exists.
    ///
    /// The file must be opened with write access for truncate to work.
    ///
    /// This is an async version of [`std::fs::OpenOptions::truncate`][std]
    ///
    /// [std]: std::fs::OpenOptions::truncate
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .write(true)
    ///         .truncate(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn truncate(&mut self, truncate: bool) -> &mut OpenOptions {
        self.0.truncate(truncate);
        self
    }

    /// Sets the option for creating a new file.
    ///
    /// This option indicates whether a new file will be created if the file
    /// does not yet already exist.
    ///
    /// In order for the file to be created, [`write`] or [`append`] access must
    /// be used.
    ///
    /// This is an async version of [`std::fs::OpenOptions::create`][std]
    ///
    /// [std]: std::fs::OpenOptions::create
    /// [`write`]: OpenOptions::write
    /// [`append`]: OpenOptions::append
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .write(true)
    ///         .create(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn create(&mut self, create: bool) -> &mut OpenOptions {
        self.0.create(create);
        self
    }

    /// Sets the option to always create a new file.
    ///
    /// This option indicates whether a new file will be created.  No file is
    /// allowed to exist at the target location, also no (dangling) symlink.
    ///
    /// This option is useful because it is atomic. Otherwise between checking
    /// whether a file exists and creating a new one, the file may have been
    /// created by another process (a TOCTOU race condition / attack).
    ///
    /// If `.create_new(true)` is set, [`.create()`] and [`.truncate()`] are
    /// ignored.
    ///
    /// The file must be opened with write or append access in order to create a
    /// new file.
    ///
    /// This is an async version of [`std::fs::OpenOptions::create_new`][std]
    ///
    /// [std]: std::fs::OpenOptions::create_new
    /// [`.create()`]: OpenOptions::create
    /// [`.truncate()`]: OpenOptions::truncate
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new()
    ///         .write(true)
    ///         .create_new(true)
    ///         .open("foo.txt")
    ///         .await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn create_new(&mut self, create_new: bool) -> &mut OpenOptions {
        self.0.create_new(create_new);
        self
    }

    /// Opens a file at `path` with the options specified by `self`.
    ///
    /// This is an async version of [`std::fs::OpenOptions::open`][std]
    ///
    /// [std]: std::fs::OpenOptions::open
    ///
    /// # Errors
    ///
    /// This function will return an error under a number of different
    /// circumstances. Some of these error conditions are listed here, together
    /// with their [`ErrorKind`]. The mapping to [`ErrorKind`]s is not part of
    /// the compatibility contract of the function, especially the `Other` kind
    /// might change to more specific kinds in the future.
    ///
    /// * [`NotFound`]: The specified file does not exist and neither `create`
    ///   or `create_new` is set.
    /// * [`NotFound`]: One of the directory components of the file path does
    ///   not exist.
    /// * [`PermissionDenied`]: The user lacks permission to get the specified
    ///   access rights for the file.
    /// * [`PermissionDenied`]: The user lacks permission to open one of the
    ///   directory components of the specified path.
    /// * [`AlreadyExists`]: `create_new` was specified and the file already
    ///   exists.
    /// * [`InvalidInput`]: Invalid combinations of open options (truncate
    ///   without write access, no access mode set, etc.).
    /// * [`Other`]: One of the directory components of the specified file path
    ///   was not, in fact, a directory.
    /// * [`Other`]: Filesystem-level errors: full disk, write permission
    ///   requested on a read-only file system, exceeded disk quota, too many
    ///   open files, too long filename, too many symbolic links in the
    ///   specified path (Unix-like systems only), etc.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let file = OpenOptions::new().open("foo.txt").await?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// [`ErrorKind`]: std::io::ErrorKind
    /// [`AlreadyExists`]: std::io::ErrorKind::AlreadyExists
    /// [`InvalidInput`]: std::io::ErrorKind::InvalidInput
    /// [`NotFound`]: std::io::ErrorKind::NotFound
    /// [`Other`]: std::io::ErrorKind::Other
    /// [`PermissionDenied`]: std::io::ErrorKind::PermissionDenied
    pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let opts = self.0.clone();

        let std = asyncify(move || opts.open(path)).await?;
        Ok(File::from_std(std))
    }
}

impl From<std::fs::OpenOptions> for OpenOptions {
    fn from(options: std::fs::OpenOptions) -> OpenOptions {
        OpenOptions(options)
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}
