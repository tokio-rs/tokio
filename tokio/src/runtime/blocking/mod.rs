//! Abstracts out the APIs necessary to `Runtime` for integrating the blocking
//! pool. When the `blocking` feature flag is **not** enabled, these APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

cfg_blocking_impl! {
    mod pool;
    pub(crate) use pool::{spawn_blocking, try_spawn_blocking, BlockingPool, Spawner};

    mod schedule;
    mod shutdown;
    mod task;

    use crate::runtime::Builder;

    pub(crate) fn create_blocking_pool(builder: &Builder, thread_cap: usize) -> BlockingPool {
        BlockingPool::new(builder, thread_cap)

    }
}

cfg_not_blocking_impl! {
    use crate::runtime::Builder;
    use std::time::Duration;

    #[derive(Debug, Clone)]
    pub(crate) struct BlockingPool {}

    pub(crate) use BlockingPool as Spawner;

    pub(crate) fn create_blocking_pool(_builder: &Builder, _thread_cap: usize) -> BlockingPool {
        BlockingPool {}
    }

    impl BlockingPool {
        pub(crate) fn spawner(&self) -> &BlockingPool {
            self
        }

        pub(crate) fn shutdown(&mut self, _duration: Option<Duration>) {
        }
    }
}
