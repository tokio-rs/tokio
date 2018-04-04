//! A work-stealing based thread pool for executing futures.

#![doc(html_root_url = "https://docs.rs/tokio-threadpool/0.1.2")]
#![deny(warnings, missing_docs, missing_debug_implementations)]

extern crate tokio_executor;
extern crate futures;
extern crate crossbeam_deque as deque;
extern crate num_cpus;
extern crate rand;

#[macro_use]
extern crate log;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

pub mod park;

mod builder;
mod callback;
mod config;
#[cfg(feature = "unstable-futures")]
mod futures2_wake;
mod notifier;
mod pool;
mod sender;
mod shutdown;
mod shutdown_task;
mod sleep_stack;
mod task;
mod thread_pool;
mod worker;

pub use builder::Builder;
pub use sender::Sender;
pub use shutdown::Shutdown;
pub use thread_pool::ThreadPool;
pub use worker::Worker;
