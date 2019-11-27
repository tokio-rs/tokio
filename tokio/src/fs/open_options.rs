use crate::fs::{asyncify, File};

use std::io;
use std::path::Path;

/// Options and flags which can be used to configure how a file is opened.
///
/// This is a specialized version of [`std::fs::OpenOptions`] for usage from
/// the Tokio runtime.
///
/// `From<std::fs::OpenOptions>` is implemented for more advanced configuration
/// than the methods provided here.
///
/// [`std::fs::OpenOptions`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html
#[derive(Clone, Debug)]
pub struct OpenOptions(std::fs::OpenOptions);

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
        OpenOptions(std::fs::OpenOptions::new())
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

    /// Opens a file at `path` with the options specified by `self`.
    ///
    /// # Errors
    ///
    /// `OpenOptionsFuture` results in an error if called from outside of the
    /// Tokio runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.open
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
