//! Windows-specific types for signal handling.
//!
//! This module is only defined on Windows and allows receiving "ctrl-c"
//! and "ctrl-break" notifications. These events are listened for via the
//! `SetConsoleCtrlHandler` function which receives events of the type
//! `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`.

#![cfg(any(windows, docsrs))]
#![cfg_attr(docsrs, doc(cfg(all(windows, feature = "signal"))))]

use std::io;
use std::task::{Context, Poll};

use self::os_impl::Event;

#[cfg(windows)]
mod os_impl;
#[cfg(windows)]
pub(crate) use self::os_impl::{OsExtraData, OsStorage};

#[cfg(not(windows))]
mod os_impl {
    use std::io;
    pub(crate) struct Event {
        pub(super) inner: crate::signal::RxFuture,
    }
    impl Event {
        pub(super) fn new_ctrl_c() -> io::Result<Self> {
            panic!()
        }

        pub(super) fn new_ctrl_break() -> io::Result<Self> {
            panic!()
        }
    }
}

/// Creates a new stream which receives "ctrl-c" notifications sent to the
/// process.
///
/// # Examples
///
/// ```rust,no_run
/// use tokio::signal::windows::ctrl_c;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // An infinite stream of CTRL-C events.
///     let mut stream = ctrl_c()?;
///
///     // Print whenever a CTRL-C event is received.
///     for countdown in (0..3).rev() {
///         stream.recv().await;
///         println!("got CTRL-C. {} more to exit", countdown);
///     }
///
///     Ok(())
/// }
/// ```
pub fn ctrl_c() -> io::Result<CtrlC> {
    Event::new_ctrl_c().map(|inner| CtrlC { inner })
}

/// Represents a stream which receives "ctrl-c" notifications sent to the process
/// via `SetConsoleCtrlHandler`.
///
/// A notification to this process notifies *all* streams listening for
/// this event. Moreover, the notifications **are coalesced** if they aren't processed
/// quickly enough. This means that if two notifications are received back-to-back,
/// then the stream may only receive one item about the two notifications.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct CtrlC {
    inner: Event,
}

impl CtrlC {
    /// Receives the next signal notification event.
    ///
    /// `None` is returned if no more events can be received by this stream.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tokio::signal::windows::ctrl_c;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // An infinite stream of CTRL-C events.
    ///     let mut stream = ctrl_c()?;
    ///
    ///     // Print whenever a CTRL-C event is received.
    ///     for countdown in (0..3).rev() {
    ///         stream.recv().await;
    ///         println!("got CTRL-C. {} more to exit", countdown);
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<()> {
        self.inner.inner.recv().await
    }

    /// Polls to receive the next signal notification event, outside of an
    /// `async` context.
    ///
    /// `None` is returned if no more events can be received by this stream.
    ///
    /// # Examples
    ///
    /// Polling from a manually implemented future
    ///
    /// ```rust,no_run
    /// use std::pin::Pin;
    /// use std::future::Future;
    /// use std::task::{Context, Poll};
    /// use tokio::signal::windows::CtrlC;
    ///
    /// struct MyFuture {
    ///     ctrl_c: CtrlC,
    /// }
    ///
    /// impl Future for MyFuture {
    ///     type Output = Option<()>;
    ///
    ///     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    ///         println!("polling MyFuture");
    ///         self.ctrl_c.poll_recv(cx)
    ///     }
    /// }
    /// ```
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.inner.inner.poll_recv(cx)
    }
}

/// Represents a stream which receives "ctrl-break" notifications sent to the process
/// via `SetConsoleCtrlHandler`.
///
/// A notification to this process notifies *all* streams listening for
/// this event. Moreover, the notifications **are coalesced** if they aren't processed
/// quickly enough. This means that if two notifications are received back-to-back,
/// then the stream may only receive one item about the two notifications.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct CtrlBreak {
    inner: Event,
}

impl CtrlBreak {
    /// Receives the next signal notification event.
    ///
    /// `None` is returned if no more events can be received by this stream.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tokio::signal::windows::ctrl_break;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // An infinite stream of CTRL-BREAK events.
    ///     let mut stream = ctrl_break()?;
    ///
    ///     // Print whenever a CTRL-BREAK event is received.
    ///     loop {
    ///         stream.recv().await;
    ///         println!("got signal CTRL-BREAK");
    ///     }
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<()> {
        self.inner.inner.recv().await
    }

    /// Polls to receive the next signal notification event, outside of an
    /// `async` context.
    ///
    /// `None` is returned if no more events can be received by this stream.
    ///
    /// # Examples
    ///
    /// Polling from a manually implemented future
    ///
    /// ```rust,no_run
    /// use std::pin::Pin;
    /// use std::future::Future;
    /// use std::task::{Context, Poll};
    /// use tokio::signal::windows::CtrlBreak;
    ///
    /// struct MyFuture {
    ///     ctrl_break: CtrlBreak,
    /// }
    ///
    /// impl Future for MyFuture {
    ///     type Output = Option<()>;
    ///
    ///     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    ///         println!("polling MyFuture");
    ///         self.ctrl_break.poll_recv(cx)
    ///     }
    /// }
    /// ```
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.inner.inner.poll_recv(cx)
    }
}

/// Creates a new stream which receives "ctrl-break" notifications sent to the
/// process.
///
/// # Examples
///
/// ```rust,no_run
/// use tokio::signal::windows::ctrl_break;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // An infinite stream of CTRL-BREAK events.
///     let mut stream = ctrl_break()?;
///
///     // Print whenever a CTRL-BREAK event is received.
///     loop {
///         stream.recv().await;
///         println!("got signal CTRL-BREAK");
///     }
/// }
/// ```
pub fn ctrl_break() -> io::Result<CtrlBreak> {
    Event::new_ctrl_break().map(|inner| CtrlBreak { inner })
}
