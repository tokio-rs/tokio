use super::Runtime;
use std::future::Future;

impl Runtime {
    /// Like `block_on`, but takes an `async` block
    pub fn block_on_async<F>(&mut self, future: F) -> F::Output
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        use crate::async_await::map_result;
        use tokio_futures::compat::backward;

        let future = backward::Compat::new(map_result(future));
        match self.block_on(future) {
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
    }
}
