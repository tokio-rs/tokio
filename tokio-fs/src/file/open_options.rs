use super::File;

use futures::{Future, Poll};

use std::fs::OpenOptions as StdOpenOptions;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as UnixOpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as WindowsOpenOptionsExt;
use std::path::Path;

/// Future returned by `OpenOptions::open` and resolves to a `File` instance.
#[derive(Debug)]
pub struct OpenOptionsFuture<P> {
    options: StdOpenOptions,
    path: P,
}

impl<P> OpenOptionsFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(options: StdOpenOptions, path: P) -> Self {
        OpenOptionsFuture { options, path }
    }
}

impl<P> Future for OpenOptionsFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let std = try_ready!(::blocking_io(|| {
            self.options.open(&self.path)
        }));

        let file = File::from_std(std);
        Ok(file.into())
    }
}

/// Options and flags which can be used to configure how a file is opened.
///
/// This is a specialized version of [`std::fs::OpenOptions`] for usage from
/// the Tokio runtime.
///
/// [`std::fs::OpenOptions`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html
#[derive(Clone, Debug)]
pub struct OpenOptions(StdOpenOptions);

impl OpenOptions {
    /// Creates a blank new set of options ready for configuration.
    ///
    /// All options are initially set to `false`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use tokio::fs::OpenOptions;
    ///
    /// let mut options = OpenOptions::new();
    /// let future = options.read(true).open("foo.txt");
    /// ```
    pub fn new() -> OpenOptions {
        OpenOptions(StdOpenOptions::new())
    }

    /// See the underlying [`read`] call for details.
    ///
    /// [`read`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.read
    pub fn read(&mut self, read: bool) -> &mut OpenOptions {
        self.0.read(read);
        self
    }

    /// See the underlying [`write`] call for details.
    ///
    /// [`write`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.write
    pub fn write(&mut self, write: bool) -> &mut OpenOptions {
        self.0.write(write);
        self
    }

    /// See the underlying [`append`] call for details.
    ///
    /// [`append`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.append
    pub fn append(&mut self, append: bool) -> &mut OpenOptions {
        self.0.append(append);
        self
    }

    /// See the underlying [`truncate`] call for details.
    ///
    /// [`truncate`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.truncate
    pub fn truncate(&mut self, truncate: bool) -> &mut OpenOptions {
        self.0.truncate(truncate);
        self
    }

    /// See the underlying [`create`] call for details.
    ///
    /// [`create`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create
    pub fn create(&mut self, create: bool) -> &mut OpenOptions {
        self.0.create(create);
        self
    }

    /// See the underlying [`create_new`] call for details.
    ///
    /// [`create_new`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new
    pub fn create_new(&mut self, create_new: bool) -> &mut OpenOptions {
        self.0.create_new(create_new);
        self
    }

    /// See the underlying [`mode`] call for details.
    ///
    /// [`mode`]: https://doc.rust-lang.org/beta/std/os/unix/fs/trait.OpenOptionsExt.html#tymethod.mode
    #[cfg(unix)]
    pub fn mode(&mut self, mode: u32) -> &mut OpenOptions {
        self.0.mode(mode);
        self
    }

    /// See the underlying [`custom_flags`] call for details.
    ///
    /// [`custom_flags`]: https://doc.rust-lang.org/beta/std/os/unix/fs/trait.OpenOptionsExt.html#tymethod.custom_flags
    #[cfg(unix)]
    pub fn custom_flags(&mut self, flags: i32) -> &mut OpenOptions {
        self.0.custom_flags(flags);
        self
    }

    /// See the underlying [`access_mode`] call for details.
    ///
    /// [`access_mode`]: https://doc.rust-lang.org/beta/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.access_mode
    #[cfg(windows)]
    pub fn access_mode(&mut self, access: u32) -> &mut OpenOptions {
        self.0.access_mode(access);
        self
    }

    /// See the underlying [`share_mode`] call for details.
    ///
    /// [`share_mode`]: https://doc.rust-lang.org/beta/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode
    #[cfg(windows)]
    pub fn share_mode(&mut self, share: u32) -> &mut OpenOptions {
        self.0.share_mode(share);
        self
    }

    /// See the underlying [`custom_flags`] call for details.
    ///
    /// [`custom_flags`]: https://doc.rust-lang.org/beta/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.custom_flags
    #[cfg(windows)]
    pub fn custom_flags(&mut self, flags: u32) -> &mut OpenOptions {
        self.0.custom_flags(flags);
        self
    }

    /// See the underlying [`attributes`] call for details.
    ///
    /// [`attributes`]: https://doc.rust-lang.org/beta/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.attributes
    #[cfg(windows)]
    pub fn attributes(&mut self, attributes: u32) -> &mut OpenOptions {
        self.0.attributes(attributes);
        self
    }

    /// See the underlying [`security_qos_flags`] call for details.
    ///
    /// [`security_qos_flags`]: https://doc.rust-lang.org/beta/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.security_qos_flags
    #[cfg(windows)]
    pub fn security_qos_flags(&mut self, flags: u32) -> &mut OpenOptions {
        self.0.security_qos_flags(flags);
        self
    }

    /// Opens a file at `path` with the options specified by `self`.
    ///
    /// # Errors
    ///
    /// `OpenOptionsFuture` results in an error if called from outside of the
    /// Tokio runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.open
    pub fn open<P>(&self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static
    {
        self.clone().into_open(path)
    }

    /// Opens a file at `path` with the options specified by `self`, consuming
    /// `self`.
    ///
    /// # Errors
    ///
    /// `OpenOptionsFuture` results in an error if called from outside of the
    /// Tokio runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.open
    pub fn into_open<P>(self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static
    {
        OpenOptionsFuture::new(self.0, path)
    }
}
