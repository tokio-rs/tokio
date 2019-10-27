//! Threadpool

mod builder;
pub use self::builder::Builder;

mod current;

mod idle;
use self::idle::Idle;

mod join;
pub use self::join::JoinHandle;

mod owned;
use self::owned::Owned;

mod park;

mod pool;
pub use self::pool::ThreadPool;

mod queue;

mod spawner;
pub use self::spawner::Spawner;

mod set;

mod shared;
use self::shared::Shared;

mod shutdown;

mod worker;

/// Unit tests
#[cfg(test)]
mod tests;

// Re-export `task::Error`
pub use crate::executor::task::Error;

// These exports are used in tests
#[cfg(test)]
#[allow(warnings)]
pub(crate) use self::worker::create_set as create_pool;

pub(crate) type BoxFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

#[cfg(not(loom))]
const LOCAL_QUEUE_CAPACITY: usize = 256;

// Shrink the size of the local queue when using loom. This shouldn't impact
// logic, but allows loom to test more edge cases in a reasonable a mount of
// time.
#[cfg(loom)]
const LOCAL_QUEUE_CAPACITY: usize = 2;
