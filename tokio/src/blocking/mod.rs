//! Perform blocking operations from an asynchronous context.

mod pool;
pub(crate) use self::pool::{BlockingPool, Spawner, spawn_blocking};

mod schedule;
mod task;
