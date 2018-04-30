/// Single-threaded runtime provides a way to start reactor
/// and executor on the same thread.
///
/// The single-threaded runtime is not `Send` and cannot be
/// safely moved to other threads.
///
/// # Examples
///
/// Creating a new `Runtime` with default configuration values.
///
/// ```
/// use tokio::runtime::current_thread::Runtime;
/// use tokio::prelude::*;
///
/// let mut runtime = Runtime::new().unwrap();
///
/// // Use the runtime...
/// // runtime.block_on(f); // where f is a future
/// ```

mod builder;
mod runtime;

pub use self::builder::Builder;
pub use self::runtime::Runtime;
