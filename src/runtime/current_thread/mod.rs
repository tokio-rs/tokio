//! A runtime implementation that runs everything on the current thread.
//!
//! [`current_thread::Runtime`][rt] is similar to the primary
//! [`Runtime`][concurrent-rt] except that it runs all components on the current
//! thread instead of using a thread pool. This means that it is able to spawn
//! futures that do not implement `Send`.
//!
//! Same as the default [`Runtime`][concurrent-rt], the
//! [`current_thread::Runtime`][rt] includes:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
//! * A [timer] for scheduling work to run after a set period of time.
//!
//! Note that [`current_thread::Runtime`][rt] does not implement `Send` itself
//! and cannot be safely moved to other threads.
//!
//! # Examples
//!
//! Creating a new `Runtime` with default configuration values.
//!
//! ```
//! use tokio::runtime::current_thread::Runtime;
//! use tokio::prelude::*;
//!
//! let mut runtime = Runtime::new().unwrap();
//!
//! // Use the runtime...
//! // runtime.block_on(f); // where f is a future
//! ```
//!
//! [rt]: struct.Runtime.html
//! [concurrent-rt]: ../struct.Runtime.html

mod builder;
mod runtime;

pub use self::builder::Builder;
pub use self::runtime::Runtime;
