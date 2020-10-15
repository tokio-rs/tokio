use crate::fs::dir_builder::DirBuilder;

/// Unix-specific extensions to [`DirBuilder`].
///
/// [`DirBuilder`]: crate::fs::DirBuilder
pub trait DirBuilderExt: sealed::Sealed {
    /// Sets the mode to create new directories with.
    ///
    /// This option defaults to 0o777.
    ///
    /// # Examples
    ///
    ///
    /// ```no_run
    /// use tokio::fs::DirBuilder;
    /// use tokio::fs::os::unix::DirBuilderExt;
    ///
    /// let mut builder = DirBuilder::new();
    /// builder.mode(0o775);
    /// ```
    fn mode(&mut self, mode: u32) -> &mut Self;
}

impl DirBuilderExt for DirBuilder {
    fn mode(&mut self, mode: u32) -> &mut Self {
        self.mode = Some(mode);
        self
    }
}

impl sealed::Sealed for DirBuilder {}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait Sealed {}
}
