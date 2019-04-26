use super::Runtime;
use std::future::Future;

impl Runtime {
    /// Like `block_on`, but takes an `async` block
    pub fn block_on_async<F>(&mut self, future: F) -> F::Output
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        use tokio_futures::compat;

        match self.block_on(compat::infallible_into_01(future)) {
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
    }
}
