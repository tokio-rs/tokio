use runtime::{Inner, Runtime};

use reactor::Reactor;

use std::io;

use tokio_threadpool::Builder as ThreadPoolBuilder;



/// Builds Tokio Runtime with custom configuration values.
///
/// Methods can be chanined in order to set the configuration values. The
/// Runtime is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
/// See function level documentation for details on the various configuration
/// settings.
///
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// # extern crate tokio;
/// # extern crate tokio_threadpool;
/// # use tokio::runtime::Builder;
///
/// # pub fn main() {
/// let runtime = Builder::new()
///     .threadpool_builder(
///         // Create a thread pool with some configuration values
///         tokio_threadpool::Builder::new()
///             .name_prefix("my-runtime-worker-")
///             .pool_size(4)
///             .clone()
///         )
///     .build();
/// // ... call runtime.run(...)
/// # let _ = runtime;
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Builder {
    /// Thread pool specific builder
    threadpool_builder: ThreadPoolBuilder,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {

        Builder {
            threadpool_builder: ThreadPoolBuilder::new(),
        }
    }

    /// Set builder to set up the thread pool instance.
    pub fn threadpool_builder(&mut self, val: ThreadPoolBuilder) -> &mut Self {
        self.threadpool_builder = val;
        self
    }

    /// Create the configured `Runtime`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::runtime::Builder;
    /// # pub fn main() {
    /// let runtime = Builder::new().build();
    /// // ... call runtime.run(...)
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        // Spawn a reactor on a background thread.
        let reactor = Reactor::new()?.background()?;

        // Get a handle to the reactor.
        let handle = reactor.handle().clone();

        let pool = self.threadpool_builder
            .around_worker(move |w, enter| {
                ::tokio_reactor::with_default(&handle, enter, |_| {
                    w.run();
                });
            })
            .build();

        Ok(Runtime {
            inner: Some(Inner {
                reactor,
                pool,
            }),
        })
    }
}
