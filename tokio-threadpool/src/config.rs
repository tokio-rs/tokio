use callback::Callback;

use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// Thread pool specific configuration values
#[derive(Clone)]
pub(crate) struct Config {
    pub keep_alive: Option<Duration>,
    // Used to configure a worker thread
    pub name_prefix: Option<String>,
    pub stack_size: Option<usize>,
    pub around_worker: Option<Callback>,
    pub after_start: Option<Arc<Fn() + Send + Sync>>,
    pub before_stop: Option<Arc<Fn() + Send + Sync>>,
    pub panic_handler: Option<Arc<Fn(Box<Any + Send>) + Send + Sync>>,
}

/// Max number of workers that can be part of a pool. This is the most that can
/// fit in the scheduler state. Note, that this is the max number of **active**
/// threads. There can be more standby threads.
pub(crate) const MAX_WORKERS: usize = 1 << 15;

impl fmt::Debug for Config {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Config")
            .field("keep_alive", &self.keep_alive)
            .field("name_prefix", &self.name_prefix)
            .field("stack_size", &self.stack_size)
            .finish()
    }
}
