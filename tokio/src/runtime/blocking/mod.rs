//! Abstracts out the APIs necessary to `Runtime` for integrating the blocking
//! pool. When the `blocking` feature flag is **not** enabled. These APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

cfg_blocking_impl! {
    mod pool;
    pub(crate) use pool::{spawn_blocking, BlockingPool, Spawner};

    mod schedule;
    mod shutdown;
    mod task;

    use crate::runtime::{self, Builder, io, time};

    pub(crate) fn create_blocking_pool(
        builder: &Builder,
        spawner: &runtime::Spawner,
        io: &io::Handle,
        time: &time::Handle,
        clock: &time::Clock,
    ) -> BlockingPool {
        BlockingPool::new(
            builder.thread_name.clone(),
            builder.thread_stack_size,
            builder.after_start.clone(),
            builder.before_stop.clone(),
            spawner.clone(),
            io.clone(),
            time.clone(),
            clock.clone())

    }
}

cfg_not_blocking_impl! {
    use crate::runtime::{self, io, time, Builder};

    #[derive(Debug, Clone)]
    pub(crate) struct BlockingPool {}

    pub(crate) use BlockingPool as Spawner;

    pub(crate) fn create_blocking_pool(
        _builder: &Builder,
        _spawner: &runtime::Spawner,
        _io: &io::Handle,
        _time: &time::Handle,
        _clock: &time::Clock,
    ) -> BlockingPool {
        BlockingPool {}
    }

    impl BlockingPool {
        pub(crate) fn spawner(&self) -> &BlockingPool {
            self
        }

        pub(crate) fn enter<F, R>(&self, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            f()
        }
    }
}
