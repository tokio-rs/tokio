use crate::callback::Callback;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// Thread pool specific configuration values
#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) keep_alive: Option<Duration>,
    // Used to configure a worker thread
    pub(crate) name_prefix: Option<String>,
    pub(crate) stack_size: Option<usize>,
    pub(crate) around_worker: Option<Callback>,
    pub(crate) after_start: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) before_stop: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) panic_handler: Option<PanicHandler>,
}

// Define type alias to avoid clippy::type_complexity.
type PanicHandler = Arc<dyn Fn(Box<dyn Any + Send>) + Send + Sync>;

/// Max number of workers that can be part of a pool. This is the most that can
/// fit in the scheduler state. Note, that this is the max number of **active**
/// threads. There can be more standby threads.
pub(crate) const MAX_WORKERS: usize = 1 << 15;

impl fmt::Debug for Config {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Config")
            .field("keep_alive", &self.keep_alive)
            .field("name_prefix", &self.name_prefix)
            .field("stack_size", &self.stack_size)
            .finish()
    }
}
