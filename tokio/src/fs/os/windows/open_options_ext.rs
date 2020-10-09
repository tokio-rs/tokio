use crate::fs::open_options::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;

/// Unix-specific extensions to [`fs::OpenOptions`].
///
/// This mirrors the definition of [`std::os::windows::fs::OpenOptionsExt`].
///
/// [`fs::OpenOptions`]: crate::fs::OpenOptions
/// [`std::os::windows::fs::OpenOptionsExt`]: std::os::windows::fs::OpenOptionsExt
pub trait OpenOptionsExt: sealed::Sealed {
    /// Overrides the `dwDesiredAccess` argument to the call to [`CreateFile`]
    /// with the specified value.
    ///
    /// This will override the `read`, `write`, and `append` flags on the
    /// `OpenOptions` structure. This method provides fine-grained control over
    /// the permissions to read, write and append data, attributes (like hidden
    /// and system), and extended attributes.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::windows::OpenOptionsExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// // Open without read and write permission, for example if you only need
    /// // to call `stat` on the file
    /// let file = OpenOptions::new().access_mode(0).open("foo.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    fn access_mode(&mut self, access: u32) -> &mut Self;

    /// Overrides the `dwShareMode` argument to the call to [`CreateFile`] with
    /// the specified value.
    ///
    /// By default `share_mode` is set to
    /// `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`. This allows
    /// other processes to read, write, and delete/rename the same file
    /// while it is open. Removing any of the flags will prevent other
    /// processes from performing the corresponding operation until the file
    /// handle is closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::windows::OpenOptionsExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// // Do not allow others to read or modify this file while we have it open
    /// // for writing.
    /// let file = OpenOptions::new()
    ///     .write(true)
    ///     .share_mode(0)
    ///     .open("foo.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    fn share_mode(&mut self, val: u32) -> &mut Self;

    /// Sets extra flags for the `dwFileFlags` argument to the call to
    /// [`CreateFile2`] to the specified value (or combines it with
    /// `attributes` and `security_qos_flags` to set the `dwFlagsAndAttributes`
    /// for [`CreateFile`]).
    ///
    /// Custom flags can only set flags, not remove flags set by Rust's options.
    /// This option overwrites any previously set custom flags.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi::um::winbase::FILE_FLAG_DELETE_ON_CLOSE;
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::windows::OpenOptionsExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let file = OpenOptions::new()
    ///     .create(true)
    ///     .write(true)
    ///     .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
    ///     .open("foo.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    /// [`CreateFile2`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2
    fn custom_flags(&mut self, flags: u32) -> &mut Self;

    /// Sets the `dwFileAttributes` argument to the call to [`CreateFile2`] to
    /// the specified value (or combines it with `custom_flags` and
    /// `security_qos_flags` to set the `dwFlagsAndAttributes` for
    /// [`CreateFile`]).
    ///
    /// If a _new_ file is created because it does not yet exist and
    /// `.create(true)` or `.create_new(true)` are specified, the new file is
    /// given the attributes declared with `.attributes()`.
    ///
    /// If an _existing_ file is opened with `.create(true).truncate(true)`, its
    /// existing attributes are preserved and combined with the ones declared
    /// with `.attributes()`.
    ///
    /// In all other cases the attributes get ignored.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi::um::winnt::FILE_ATTRIBUTE_HIDDEN;
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::windows::OpenOptionsExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let file = OpenOptions::new()
    ///     .write(true)
    ///     .create(true)
    ///     .attributes(FILE_ATTRIBUTE_HIDDEN)
    ///     .open("foo.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    /// [`CreateFile2`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2
    fn attributes(&mut self, val: u32) -> &mut Self;

    /// Sets the `dwSecurityQosFlags` argument to the call to [`CreateFile2`] to
    /// the specified value (or combines it with `custom_flags` and `attributes`
    /// to set the `dwFlagsAndAttributes` for [`CreateFile`]).
    ///
    /// By default `security_qos_flags` is not set. It should be specified when
    /// opening a named pipe, to control to which degree a server process can
    /// act on behalf of a client process (security impersonation level).
    ///
    /// When `security_qos_flags` is not set, a malicious program can gain the
    /// elevated privileges of a privileged Rust process when it allows opening
    /// user-specified paths, by tricking it into opening a named pipe. So
    /// arguably `security_qos_flags` should also be set when opening arbitrary
    /// paths. However the bits can then conflict with other flags, specifically
    /// `FILE_FLAG_OPEN_NO_RECALL`.
    ///
    /// For information about possible values, see [Impersonation Levels] on the
    /// Windows Dev Center site. The `SECURITY_SQOS_PRESENT` flag is set
    /// automatically when using this method.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi::um::winbase::SECURITY_IDENTIFICATION;
    /// use tokio::fs::OpenOptions;
    /// use tokio::fs::os::windows::OpenOptionsExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let file = OpenOptions::new()
    ///     .write(true)
    ///     .create(true)
    ///
    ///     // Sets the flag value to `SecurityIdentification`.
    ///     .security_qos_flags(SECURITY_IDENTIFICATION)
    ///
    ///     .open(r"\\.\pipe\MyPipe").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    /// [`CreateFile2`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2
    /// [Impersonation Levels]:
    ///     https://docs.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-security_impersonation_level
    fn security_qos_flags(&mut self, flags: u32) -> &mut Self;
}

impl OpenOptionsExt for OpenOptions {
    fn access_mode(&mut self, access: u32) -> &mut OpenOptions {
        self.as_inner_mut().access_mode(access);
        self
    }

    fn share_mode(&mut self, share: u32) -> &mut OpenOptions {
        self.as_inner_mut().share_mode(share);
        self
    }

    fn custom_flags(&mut self, flags: u32) -> &mut OpenOptions {
        self.as_inner_mut().custom_flags(flags);
        self
    }

    fn attributes(&mut self, attributes: u32) -> &mut OpenOptions {
        self.as_inner_mut().attributes(attributes);
        self
    }

    fn security_qos_flags(&mut self, flags: u32) -> &mut OpenOptions {
        self.as_inner_mut().security_qos_flags(flags);
        self
    }
}

impl sealed::Sealed for OpenOptions {}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait Sealed {}
}
