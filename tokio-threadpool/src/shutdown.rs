use pool::Pool;
use sender::Sender;

use futures::{Future, Poll, Async};
#[cfg(feature = "unstable-futures")]
use futures2;

/// Future that resolves when the thread pool is shutdown.
///
/// A `ThreadPool` is shutdown once all the worker have drained their queues and
/// shutdown their threads.
///
/// `Shutdown` is returned by [`shutdown`], [`shutdown_on_idle`], and
/// [`shutdown_now`].
///
/// [`shutdown`]: struct.ThreadPool.html#method.shutdown
/// [`shutdown_on_idle`]: struct.ThreadPool.html#method.shutdown_on_idle
/// [`shutdown_now`]: struct.ThreadPool.html#method.shutdown_now
#[derive(Debug)]
pub struct Shutdown {
    pub(crate) inner: Sender,
}

impl Shutdown {
    fn inner(&self) -> &Pool {
        &*self.inner.inner
    }
}

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        use futures::task;

        self.inner().shutdown_task.task1.register_task(task::current());

        if !self.inner().is_shutdown() {
            return Ok(Async::NotReady);
        }

        Ok(().into())
    }
}

#[cfg(feature = "unstable-futures")]
impl futures2::Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), ()> {
        trace!("Shutdown::poll");

        self.inner().shutdown_task.task2.register(cx.waker());

        if 0 != self.inner().num_workers.load(Acquire) {
            return Ok(futures2::Async::Pending);
        }

        Ok(().into())
    }
}
