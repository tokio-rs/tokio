use std::time::SystemTime;

/// Timestamps for a file.
///
/// This is a specialized version of [`std::fs::FileTimes`] for usage with
/// [`File::set_times`](crate::fs::File::set_times). Unlike the standard library version,
/// the timestamps set here are kept so that future backends such as `io-uring` can read them
/// without going through the standard library type.
///
/// # Examples
///
/// ```no_run
/// use tokio::fs::FileTimes;
///
/// # async fn dox() -> std::io::Result<()> {
/// let times = FileTimes::new()
///     .set_accessed(std::time::SystemTime::now())
///     .set_modified(std::time::SystemTime::now());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct FileTimes {
    accessed: Option<SystemTime>,
    modified: Option<SystemTime>,
}

impl FileTimes {
    /// Creates a new empty [`FileTimes`].
    ///
    /// Unset timestamps are left unchanged.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the last access time.
    pub fn set_accessed(mut self, t: SystemTime) -> Self {
        self.accessed = Some(t);
        self
    }

    /// Sets the last modification time.
    pub fn set_modified(mut self, t: SystemTime) -> Self {
        self.modified = Some(t);
        self
    }

    pub(crate) fn into_std(self) -> std::fs::FileTimes {
        let mut std_times = std::fs::FileTimes::new();
        if let Some(accessed) = self.accessed {
            std_times = std_times.set_accessed(accessed);
        }
        if let Some(modified) = self.modified {
            std_times = std_times.set_modified(modified);
        }
        std_times
    }
}
