use reactor::Background;
use tokio_threadpool::ThreadPool;

#[derive(Debug)]
pub(super) struct Inner {
    /// Reactor running on a background thread.
    pub reactor: Background,

    /// Task execution pool.
    pub pool: ThreadPool,
}
