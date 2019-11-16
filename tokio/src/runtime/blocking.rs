//! Abstracts out the APIs necessary to `Runtime` for integrating the blocking
//! pool. When the `blocking` feature flag is **not** enabled. These APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

pub(crate) use self::variant::*;

#[cfg(feature = "blocking")]
mod variant {
    pub(crate) use crate::blocking::BlockingPool;
    pub(crate) use crate::blocking::Spawner;

    use crate::runtime::Builder;

    pub(crate) fn create_blocking_pool(builder: &Builder) -> BlockingPool {
        BlockingPool::new(builder.thread_name.clone(), builder.thread_stack_size)
    }
}

#[cfg(not(feature = "blocking"))]
mod variant {
    use crate::runtime::Builder;

    #[derive(Debug, Clone)]
    pub(crate) struct BlockingPool {}

    pub(crate) use BlockingPool as Spawner;

    pub(crate) fn create_blocking_pool(_builder: &Builder) -> BlockingPool {
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
