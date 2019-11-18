#![cfg_attr(not(feature = "blocking"), allow(dead_code, unused_imports))]

//! Perform blocking operations from an asynchronous context.

cfg_blocking_impl! {
    mod pool;
    pub(crate) use pool::{spawn_blocking, BlockingPool, Spawner};

    mod schedule;
    mod task;
}
