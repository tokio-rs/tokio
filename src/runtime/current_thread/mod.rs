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
//! By default, [`current_thread::Runtime`][rt] does not provide a way to spawn
//! tasks from other threads. However, this can be accomplished by using a
//! [`mpsc::channel`][chan]. To do so, create a channel to send the task, then
//! spawn a task on [`current_thread::Runtime`][rt] that consumes the channel
//! messages and spawns new tasks for them.
//!
//! For example:
//!
//! ```
//! # extern crate tokio;
//! # extern crate futures;
//! use tokio::runtime::current_thread::Runtime;
//! use tokio::prelude::*;
//! use futures::sync::mpsc;
//!
//! # fn main() {
//! let mut runtime = Runtime::new().unwrap();
//! let (tx, rx) = mpsc::channel(128);
//! # tx.send(future::ok(()));
//!
//! runtime.spawn(rx.for_each(|task| {
//!     tokio::spawn(task);
//!     Ok(())
//! }).map_err(|e| panic!("channel error")));
//!
//! # /*
//! runtime.run().unwrap();
//! # */
//! # }
//! ```
//!
//! # Examples
//!
//! Creating a new `Runtime` and running a future `f` until its completion and
//! returning its result.
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
//! [chan]: https://docs.rs/futures/0.1/futures/sync/mpsc/fn.channel.html

mod runtime;

pub use self::runtime::Runtime;
