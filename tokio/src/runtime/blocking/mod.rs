//! Abstracts out the APIs necessary to `Runtime` for integrating the blocking
//! pool. When the `blocking` feature flag is **not** enabled, these APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

mod pool;
#[allow(unused_imports)]
pub(crate) use pool::{spawn_blocking, BlockingPool, BlockingTaskKind, Spawner};

mod sharded;

cfg_io_blocking! {
    pub(crate) use pool::spawn_mandatory_blocking;
}

mod schedule;
mod shutdown;
mod task;
pub(crate) use task::BlockingTask;

use crate::runtime::Builder;

pub(crate) fn create_blocking_pool(
    builder: &Builder,
    thread_cap: usize,
    scheduler_threads: usize,
) -> BlockingPool {
    BlockingPool::new(builder, thread_cap, scheduler_threads)
}
