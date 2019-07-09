#[cfg(unix)]
use crate::unix::Signal as Inner;
#[cfg(windows)]
use crate::windows::Event as Inner;
use crate::IoFuture;
use futures_core::stream::Stream;
use futures_util::future::FutureExt;
use futures_util::try_future::TryFutureExt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_reactor::Handle;

/// Represents a stream which receives "ctrl-c" notifications sent to the process.
///
/// In general signals are handled very differently across Unix and Windows, but
/// this is somewhat cross platform in terms of how it can be handled. A ctrl-c
/// event to a console process can be represented as a stream for both Windows
/// and Unix.
///
/// Note that there are a number of caveats listening for signals, and you may
/// wish to read up on the documentation in the `unix` or `windows` module to
/// take a peek.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct CtrlC {
    inner: Inner,
}

impl CtrlC {
    /// Creates a new stream which receives "ctrl-c" notifications sent to the
    /// process.
    ///
    /// This function binds to the default reactor.
    pub fn new() -> IoFuture<Self> {
        Self::with_handle(&Handle::default())
    }

    /// Creates a new stream which receives "ctrl-c" notifications sent to the
    /// process.
    ///
    /// This function binds to reactor specified by `handle`.
    pub fn with_handle(handle: &Handle) -> IoFuture<CtrlC> {
        Inner::ctrl_c(handle).map_ok(|inner| Self { inner }).boxed()
    }
}

impl Stream for CtrlC {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|item| item.map(|_| ()))
    }
}
