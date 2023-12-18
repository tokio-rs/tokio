use std::ops::{Deref, DerefMut};

use tokio::task::JoinHandle;

use futures_core::Future;

/// This is a wrapper type around JoinHandle that allows it to be dropped.
#[derive(Debug)]
pub struct DropHandle<T>(JoinHandle<T>);

impl<T> Drop for DropHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> Deref for DropHandle<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for DropHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// This function spawns a task that aborts on drop instead of lingering.
pub fn spawn_aborting<F>(future: F) -> DropHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    DropHandle(tokio::spawn(future))
}
