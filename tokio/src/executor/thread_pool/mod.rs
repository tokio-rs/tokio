//! Threadpool

mod builder;
pub use self::builder::Builder;

mod current;

mod idle;
use self::idle::Idle;

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

#[cfg(feature = "blocking")]
pub use worker::blocking;

#[cfg(not(loom))]
const LOCAL_QUEUE_CAPACITY: usize = 256;

// Shrink the size of the local queue when using loom. This shouldn't impact
// logic, but allows loom to test more edge cases in a reasonable a mount of
// time.
#[cfg(loom)]
const LOCAL_QUEUE_CAPACITY: usize = 2;
