#[cfg(unix)]
use super::unix::{self as os_impl, Signal as Inner};
#[cfg(windows)]
use super::windows::{self as os_impl, Event as Inner};

use futures_core::stream::Stream;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

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
///
/// Notably, a notification to this process notifies *all* streams listening to
/// this event. Moreover, the notifications **are coalesced** if they aren't processed
/// quickly enough. This means that if two notifications are received back-to-back,
/// then the stream may only receive one item about the two notifications.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct CtrlC {
    inner: Inner,
}

/// Creates a new stream which receives "ctrl-c" notifications sent to the
/// process.
///
/// This function binds to the default reactor.
pub fn ctrl_c() -> io::Result<CtrlC> {
    os_impl::ctrl_c().map(|inner| CtrlC { inner })
}

impl Stream for CtrlC {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
