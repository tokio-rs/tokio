//! Threadpool

mod builder;
pub(crate) use self::builder::Builder;

mod current;

mod idle;
use self::idle::Idle;

mod owned;
use self::owned::Owned;

mod pool;
pub(crate) use self::pool::ThreadPool;

mod queue;

mod spawner;
pub(crate) use self::spawner::Spawner;

mod set;

mod shared;
use self::shared::Shared;

mod shutdown;

mod worker;
#[cfg(feature = "blocking")]
pub(crate) use worker::blocking;

/// Unit tests
#[cfg(test)]
mod tests;

#[cfg(not(loom))]
const LOCAL_QUEUE_CAPACITY: usize = 256;

// Shrink the size of the local queue when using loom. This shouldn't impact
// logic, but allows loom to test more edge cases in a reasonable a mount of
// time.
#[cfg(loom)]
const LOCAL_QUEUE_CAPACITY: usize = 2;
