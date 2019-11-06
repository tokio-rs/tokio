use crate::loom::thread;
use crate::runtime::blocking::Pool;

use std::usize;

/// Builds a blocking thread pool with custom configuration values.
pub(crate) struct Builder {
    /// Thread name
    name: String,

    /// Thread stack size
    stack_size: Option<usize>,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            name: "tokio-blocking-thread".to_string(),
            stack_size: None,
        }
    }
}

impl Builder {
    /// Set name of threads spawned by the pool
    ///
    /// If this configuration is not set, then the thread will use the system
    /// default naming scheme.
    pub(crate) fn name<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.name = val.into();
        self
    }

    /// Set the stack size (in bytes) for worker threads.
    ///
    /// The actual stack size may be greater than this value if the platform
    /// specifies minimal stack size.
    ///
    /// The default stack size for spawned threads is 2 MiB, though this
    /// particular stack size is subject to change in the future.
    pub(crate) fn stack_size(&mut self, val: usize) -> &mut Self {
        self.stack_size = Some(val);
        self
    }

    pub(crate) fn build(self) -> Pool {
        let mut p = Pool::default();
        let Builder { stack_size, name } = self;
        p.new_thread = Box::new(move || {
            let mut b = thread::Builder::new().name(name.clone());
            if let Some(stack_size) = stack_size {
                b = b.stack_size(stack_size);
            }
            b
        });
        p
    }
}
