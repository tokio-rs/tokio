use callback::Callback;

use std::time::Duration;

/// Thread pool specific configuration values
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub keep_alive: Option<Duration>,
    // Used to configure a worker thread
    pub name_prefix: Option<String>,
    pub stack_size: Option<usize>,
    pub around_worker: Option<Callback>,
}

/// Max number of workers that can be part of a pool. This is the most that can
/// fit in the scheduler state. Note, that this is the max number of **active**
/// threads. There can be more standby threads.
pub(crate) const MAX_WORKERS: usize = 1 << 15;
