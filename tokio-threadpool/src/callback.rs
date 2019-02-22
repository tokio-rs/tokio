use worker::Worker;

use std::fmt;
use std::sync::Arc;

use tokio_executor::Enter;

#[derive(Clone)]
pub(crate) struct Callback {
    f: Arc<Fn(&Worker, &mut Enter) + Send + Sync>,
}

impl Callback {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&Worker, &mut Enter) + Send + Sync + 'static,
    {
        Callback { f: Arc::new(f) }
    }

    pub fn call(&self, worker: &Worker, enter: &mut Enter) {
        (self.f)(worker, enter)
    }
}

impl fmt::Debug for Callback {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Fn")
    }
}
