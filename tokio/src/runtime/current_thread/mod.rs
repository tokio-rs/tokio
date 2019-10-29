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
//! # Spawning from other threads
//!
//! While [`current_thread::Runtime`][rt] does not implement `Send` and cannot
//! safely be moved to other threads, it provides a `Handle` that can be sent
//! to other threads and allows to spawn new tasks from there.
//!
//! For example:
//!
//! ```
//! use tokio::runtime::current_thread::Runtime;
//! use std::thread;
//!
//! let runtime = Runtime::new().unwrap();
//! let handle = runtime.handle();
//!
//! thread::spawn(move || {
//!     let _ = handle.spawn(async {
//!         println!("hello world");
//!     });
//! }).join().unwrap();
//! ```
//!
//! # Examples
//!
//! Creating a new `Runtime` and running a future `f` until its completion and
//! returning its result.
//!
//! ```
//! use tokio::runtime::current_thread::Runtime;
//!
//! let runtime = Runtime::new().unwrap();
//!
//! // Use the runtime...
//! // runtime.block_on(f); // where f is a future
//! ```
//!
//! [rt]: struct.Runtime.html
//! [concurrent-rt]: ../struct.Runtime.html
//! [chan]: https://docs.rs/futures/0.1/futures/sync/mpsc/fn.channel.html
//! [reactor]: ../../reactor/struct.Reactor.html
//! [executor]: https://tokio.rs/docs/internals/runtime-model/#executors
//! [timer]: ../../timer/index.html

mod builder;
mod runtime;

pub use self::builder::Builder;
pub use self::runtime::{Handle, Runtime, RunError};
pub use crate::executor::current_thread::spawn;
pub use crate::executor::current_thread::TaskExecutor;
