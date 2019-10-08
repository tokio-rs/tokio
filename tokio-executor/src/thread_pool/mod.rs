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

mod spawner;
pub use self::spawner::Spawner;

mod queue;
mod set;

mod shared;
use self::shared::Shared;

mod shutdown;

mod thread_pool;
pub use self::thread_pool::ThreadPool;

mod worker;

// Re-export `task::Error`
pub use crate::task::Error;

// These exports are used in tests
#[cfg(test)]
#[allow(warnings)]
pub(crate) use self::worker::create_set as create_pool;

pub(crate) type BoxFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

const LOCAL_QUEUE_CAPACITY: usize = 256;
