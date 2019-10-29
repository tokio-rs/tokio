use crate::executor::park::Unpark;
use crate::executor::task;
use crate::executor::thread_pool::Shared;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An owned permission to join on a task (await its termination).
pub struct JoinHandle<T> {
    task: task::JoinHandle<T, Shared<Box<dyn Unpark>>>,
}

impl<T> JoinHandle<T>
where
    T: Send + 'static,
{
    pub(super) fn new(task: task::JoinHandle<T, Shared<Box<dyn Unpark>>>) -> JoinHandle<T> {
        JoinHandle { task }
    }
}

impl<T> Future for JoinHandle<T>
where
    T: Send + 'static,
{
    type Output = task::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
    }
}

impl<T> fmt::Debug for JoinHandle<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("JoinHandle").finish()
    }
}
