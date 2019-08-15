use super::worker::Worker;

use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Callback {
    f: Arc<dyn Fn(&Worker) + Send + Sync>,
}

impl Callback {
    pub(crate) fn new<F>(f: F) -> Self
    where
        F: Fn(&Worker) + Send + Sync + 'static,
    {
        Callback { f: Arc::new(f) }
    }

    pub(crate) fn call(&self, worker: &Worker) {
        (self.f)(worker)
    }
}

impl fmt::Debug for Callback {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Fn")
    }
}
