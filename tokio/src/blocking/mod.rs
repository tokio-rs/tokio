//! Perform blocking operations from an asynchronous context.

mod pool;
pub(crate) use self::pool::{spawn_blocking, BlockingPool, Spawner};

mod schedule;
mod task;
