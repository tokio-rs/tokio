use runtime::current_thread::Runtime;

use std::io;

/// Builds Tokio single-threaded Runtime with custom configuration values.
///
/// Methods can be chanined in order to set the configuration values. The
/// Runtime is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
#[derive(Debug)]
pub struct Builder {
}

impl Builder {
    /// Returns a new single-threaded runtime builder initialized with default
    /// configuration values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {}
    }

    /// Create a new single-threaded runtime
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::runtime::current_thread::Builder;
    /// # pub fn main() {
    /// let mut runtime = Builder::new().build().unwrap();
    /// // ... call runtime.block_on(f) where f is a future
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        Runtime::new()
    }

}
