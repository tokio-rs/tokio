//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

use crate::signal::registry::{globals, EventId, EventInfo, Globals, Init, Storage};
use crate::sync::mpsc::error::TryRecvError;
use crate::sync::mpsc::{channel, Receiver};

use libc::c_int;
use mio::net::UnixStream;
use std::io::{self, Error, ErrorKind, Write};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::task::{Context, Poll};

pub use t10::signal::unix::SignalKind;

/// A stream of events for receiving a particular type of OS signal.
///
/// In general signal handling on Unix is a pretty tricky topic, and this
/// structure is no exception! There are some important limitations to keep in
/// mind when using `Signal` streams:
///
/// * Signals handling in Unix already necessitates coalescing signals
///   together sometimes. This `Signal` stream is also no exception here in
///   that it will also coalesce signals. That is, even if the signal handler
///   for this process runs multiple times, the `Signal` stream may only return
///   one signal notification. Specifically, before `poll` is called, all
///   signal notifications are coalesced into one item returned from `poll`.
///   Once `poll` has been called, however, a further signal is guaranteed to
///   be yielded as an item.
///
///   Put another way, any element pulled off the returned stream corresponds to
///   *at least one* signal, but possibly more.
///
/// * Signal handling in general is relatively inefficient. Although some
///   improvements are possible in this crate, it's recommended to not plan on
///   having millions of signal channels open.
///
/// If you've got any questions about this feel free to open an issue on the
/// repo! New approaches to alleviate some of these limitations are always
/// appreciated!
///
/// # Caveats
///
/// The first time that a `Signal` instance is registered for a particular
/// signal kind, an OS signal-handler is installed which replaces the default
/// platform behavior when that signal is received, **for the duration of the
/// entire process**.
///
/// For example, Unix systems will terminate a process by default when it
/// receives SIGINT. But, when a `Signal` instance is created to listen for
/// this signal, the next SIGINT that arrives will be translated to a stream
/// event, and the process will continue to execute. **Even if this `Signal`
/// instance is dropped, subsequent SIGINT deliveries will end up captured by
/// Tokio, and the default platform behavior will NOT be reset**.
///
/// Thus, applications should take care to ensure the expected signal behavior
/// occurs as expected after listening for specific signals.
///
/// # Examples
///
/// Wait for SIGHUP
///
/// ```rust,no_run
/// use tokio::signal::unix::{signal, SignalKind};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // An infinite stream of hangup signals.
///     let mut stream = signal(SignalKind::hangup())?;
///
///     // Print whenever a HUP signal is received
///     loop {
///         stream.recv().await;
///         println!("got signal HUP");
///     }
/// }
/// ```
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Signal(t10::signal::unix::Signal);

/// Creates a new stream which will receive notifications when the current
/// process receives the specified signal `kind`.
///
/// This function will create a new stream which binds to the default reactor.
/// The `Signal` stream is an infinite stream which will receive
/// notifications whenever a signal is received. More documentation can be
/// found on `Signal` itself, but to reiterate:
///
/// * Signals may be coalesced beyond what the kernel already does.
/// * Once a signal handler is registered with the process the underlying
///   libc signal handler is never unregistered.
///
/// A `Signal` stream can be created for a particular signal number
/// multiple times. When a signal is received then all the associated
/// channels will receive the signal notification.
///
/// # Errors
///
/// * If the lower-level C functions fail for some reason.
/// * If the previous initialization of this specific signal failed.
/// * If the signal is one of
///   [`signal_hook::FORBIDDEN`](fn@signal_hook_registry::register#panics)
pub fn signal(kind: SignalKind) -> io::Result<Signal> {
    t10::signal::unix::signal(kind).map(Signal)
}

impl Signal {
    /// Receives the next signal notification event.
    ///
    /// `None` is returned if no more events can be received by this stream.
    ///
    /// # Examples
    ///
    /// Wait for SIGHUP
    ///
    /// ```rust,no_run
    /// use tokio::signal::unix::{signal, SignalKind};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // An infinite stream of hangup signals.
    ///     let mut stream = signal(SignalKind::hangup())?;
    ///
    ///     // Print whenever a HUP signal is received
    ///     loop {
    ///         stream.recv().await;
    ///         println!("got signal HUP");
    ///     }
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<()> {
        use crate::future::poll_fn;
        poll_fn(|cx| self.poll_recv(cx)).await
    }
}

cfg_stream! {
    impl crate::stream::Stream for Signal {
        type Item = ();

        fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<()>> {
            self.0.poll_recv(cx)
        }
    }
}

// Work around for abstracting streams internally
pub(crate) trait InternalStream: Unpin {
    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>>;
    fn try_recv(&mut self) -> Result<(), TryRecvError>;
}

impl InternalStream for Signal {
    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.poll_recv(cx)
    }

    fn try_recv(&mut self) -> Result<(), TryRecvError> {
        self.try_recv()
    }
}

pub(crate) fn ctrl_c() -> io::Result<Signal> {
    signal(SignalKind::interrupt())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn signal_enable_error_on_invalid_input() {
        signal_enable(-1, Handle::default()).unwrap_err();
    }

    #[test]
    fn signal_enable_error_on_forbidden_input() {
        signal_enable(signal_hook_registry::FORBIDDEN[0], Handle::default()).unwrap_err();
    }
}
