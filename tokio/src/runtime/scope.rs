
use std::{future::Future, marker::PhantomData, mem, pin::Pin};

use crate::{sync::RwLock, task::JoinHandle};

/// A handle that lets tasks be spawned with a non-`'static` lifetime `'s`.
#[derive(Debug)]
pub struct Scope<'s> {
    // Invariant in `'s` so callers can't widen or shrink the lifetime.
    _p: PhantomData<&'s mut &'s ()>,
    // Read-locked by every running scoped task. `close` takes the write lock
    // and therefore blocks until all scoped tasks have released their guards.
    task_guard: RwLock<()>,
}

impl<'s> Scope<'s> {
    pub(crate) fn new() -> Self {
        Self {
            _p: PhantomData,
            task_guard: RwLock::new(()),
        }
    }

    /// Spawn a task whose future may borrow data with lifetime `'s`.
    pub async fn spawn<F>(&'s self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 's,
        F::Output: Send + 'static,
    {
        // Borrow the guard with the scope's lifetime so the spawned future
        // holds the read guard for as long as it runs.
        let guard: &'s RwLock<()> = &self.task_guard;

        // we need to acquire the guard before we attempt to spawn the task, to prevent a race condition. Ideally it wouldn't be async.
        let _g = guard.read().await;

        let wrap = async move {
            let res = future.await;
            drop(_g);
            res
        };

        // The runtime requires `'static` futures, so we erase `'s` here.
        //
        // SAFETY: Before `Scope` is dropped, `close` is awaited, ensuring that no scoped task is still running. Therefore the spawned scoped task is guaranteed to be finished before the rwlock guard/resources are dropped.
        let boxed: Pin<Box<dyn Future<Output = F::Output> + Send + 's>> = Box::pin(wrap);
        let boxed: Pin<Box<dyn Future<Output = F::Output> + Send + 'static>> =
            unsafe { mem::transmute(boxed) };

        crate::task::spawn(boxed)
    }

    /// Wait for every task spawned on this scope to finish.
    ///
    /// Must be awaited before the `Scope` is dropped to uphold the soundness
    /// invariant on [`Scope::spawn`].
    pub(crate) async fn close(&self) {
        let _g = self.task_guard.write().await;
    }
}
