//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

use crate::io::{AsyncRead, PollEvented};
use crate::signal::registry::{globals, EventId, EventInfo, Globals, Init, Storage};
use crate::sync::mpsc::{channel, Receiver};

use libc::c_int;
use mio_uds::UnixStream;
use std::io::{self, Error, ErrorKind, Write};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::task::{Context, Poll};

pub(crate) type OsStorage = Vec<SignalInfo>;

// Number of different unix signals
// (FreeBSD has 33)
const SIGNUM: usize = 33;

impl Init for OsStorage {
    fn init() -> Self {
        (0..SIGNUM).map(|_| SignalInfo::default()).collect()
    }
}

impl Storage for OsStorage {
    fn event_info(&self, id: EventId) -> Option<&EventInfo> {
        self.get(id).map(|si| &si.event_info)
    }

    fn for_each<'a, F>(&'a self, f: F)
    where
        F: FnMut(&'a EventInfo),
    {
        self.iter().map(|si| &si.event_info).for_each(f)
    }
}

#[derive(Debug)]
pub(crate) struct OsExtraData {
    sender: UnixStream,
    receiver: UnixStream,
}

impl Init for OsExtraData {
    fn init() -> Self {
        let (receiver, sender) = UnixStream::pair().expect("failed to create UnixStream");

        Self { sender, receiver }
    }
}

/// Represents the specific kind of signal to listen for.
#[derive(Debug, Clone, Copy)]
pub struct SignalKind(c_int);

impl SignalKind {
    /// Allows for listening to any valid OS signal.
    ///
    /// For example, this can be used for listening for platform-specific
    /// signals.
    /// ```rust,no_run
    /// # use tokio::signal::unix::SignalKind;
    /// # let signum = -1;
    /// // let signum = libc::OS_SPECIFIC_SIGNAL;
    /// let kind = SignalKind::from_raw(signum);
    /// ```
    pub fn from_raw(signum: c_int) -> Self {
        Self(signum)
    }

    /// Represents the SIGALRM signal.
    ///
    /// On Unix systems this signal is sent when a real-time timer has expired.
    /// By default, the process is terminated by this signal.
    pub fn alarm() -> Self {
        Self(libc::SIGALRM)
    }

    /// Represents the SIGCHLD signal.
    ///
    /// On Unix systems this signal is sent when the status of a child process
    /// has changed. By default, this signal is ignored.
    pub fn child() -> Self {
        Self(libc::SIGCHLD)
    }

    /// Represents the SIGHUP signal.
    ///
    /// On Unix systems this signal is sent when the terminal is disconnected.
    /// By default, the process is terminated by this signal.
    pub fn hangup() -> Self {
        Self(libc::SIGHUP)
    }

    /// Represents the SIGINFO signal.
    ///
    /// On Unix systems this signal is sent to request a status update from the
    /// process. By default, this signal is ignored.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    pub fn info() -> Self {
        Self(libc::SIGINFO)
    }

    /// Represents the SIGINT signal.
    ///
    /// On Unix systems this signal is sent to interrupt a program.
    /// By default, the process is terminated by this signal.
    pub fn interrupt() -> Self {
        Self(libc::SIGINT)
    }

    /// Represents the SIGIO signal.
    ///
    /// On Unix systems this signal is sent when I/O operations are possible
    /// on some file descriptor. By default, this signal is ignored.
    pub fn io() -> Self {
        Self(libc::SIGIO)
    }

    /// Represents the SIGPIPE signal.
    ///
    /// On Unix systems this signal is sent when the process attempts to write
    /// to a pipe which has no reader. By default, the process is terminated by
    /// this signal.
    pub fn pipe() -> Self {
        Self(libc::SIGPIPE)
    }

    /// Represents the SIGQUIT signal.
    ///
    /// On Unix systems this signal is sent to issue a shutdown of the
    /// process, after which the OS will dump the process core.
    /// By default, the process is terminated by this signal.
    pub fn quit() -> Self {
        Self(libc::SIGQUIT)
    }

    /// Represents the SIGTERM signal.
    ///
    /// On Unix systems this signal is sent to issue a shutdown of the
    /// process. By default, the process is terminated by this signal.
    pub fn terminate() -> Self {
        Self(libc::SIGTERM)
    }

    /// Represents the SIGUSR1 signal.
    ///
    /// On Unix systems this is a user defined signal.
    /// By default, the process is terminated by this signal.
    pub fn user_defined1() -> Self {
        Self(libc::SIGUSR1)
    }

    /// Represents the SIGUSR2 signal.
    ///
    /// On Unix systems this is a user defined signal.
    /// By default, the process is terminated by this signal.
    pub fn user_defined2() -> Self {
        Self(libc::SIGUSR2)
    }

    /// Represents the SIGWINCH signal.
    ///
    /// On Unix systems this signal is sent when the terminal window is resized.
    /// By default, this signal is ignored.
    pub fn window_change() -> Self {
        Self(libc::SIGWINCH)
    }
}

pub(crate) struct SignalInfo {
    event_info: EventInfo,
    init: Once,
    initialized: AtomicBool,
}

impl Default for SignalInfo {
    fn default() -> SignalInfo {
        SignalInfo {
            event_info: Default::default(),
            init: Once::new(),
            initialized: AtomicBool::new(false),
        }
    }
}

/// Our global signal handler for all signals registered by this module.
///
/// The purpose of this signal handler is to primarily:
///
/// 1. Flag that our specific signal was received (e.g. store an atomic flag)
/// 2. Wake up driver tasks by writing a byte to a pipe
///
/// Those two operations shoudl both be async-signal safe.
fn action(globals: Pin<&'static Globals>, signal: c_int) {
    globals.record_event(signal as EventId);

    // Send a wakeup, ignore any errors (anything reasonably possible is
    // full pipe and then it will wake up anyway).
    let mut sender = &globals.sender;
    drop(sender.write(&[1]));
}

/// Enables this module to receive signal notifications for the `signal`
/// provided.
///
/// This will register the signal handler if it hasn't already been registered,
/// returning any error along the way if that fails.
fn signal_enable(signal: c_int) -> io::Result<()> {
    if signal < 0 || signal_hook_registry::FORBIDDEN.contains(&signal) {
        return Err(Error::new(
            ErrorKind::Other,
            format!("Refusing to register signal {}", signal),
        ));
    }

    let globals = globals();
    let siginfo = match globals.storage().get(signal as EventId) {
        Some(slot) => slot,
        None => return Err(io::Error::new(io::ErrorKind::Other, "signal too large")),
    };
    let mut registered = Ok(());
    siginfo.init.call_once(|| {
        registered = unsafe {
            signal_hook_registry::register(signal, move || action(globals, signal)).map(|_| ())
        };
        if registered.is_ok() {
            siginfo.initialized.store(true, Ordering::Relaxed);
        }
    });
    registered?;
    // If the call_once failed, it won't be retried on the next attempt to register the signal. In
    // such case it is not run, registered is still `Ok(())`, initialized is still `false`.
    if siginfo.initialized.load(Ordering::Relaxed) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            "Failed to register signal handler",
        ))
    }
}

#[derive(Debug)]
struct Driver {
    wakeup: PollEvented<UnixStream>,
}

impl Driver {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        // Drain the data from the pipe and maintain interest in getting more
        self.drain(cx);
        // Broadcast any signals which were received
        globals().broadcast();

        Poll::Pending
    }
}

impl Driver {
    fn new() -> io::Result<Driver> {
        // NB: We give each driver a "fresh" reciever file descriptor to avoid
        // the issues described in alexcrichton/tokio-process#42.
        //
        // In the past we would reuse the actual receiver file descriptor and
        // swallow any errors around double registration of the same descriptor.
        // I'm not sure if the second (failed) registration simply doesn't end up
        // receiving wake up notifications, or there could be some race condition
        // when consuming readiness events, but having distinct descriptors for
        // distinct PollEvented instances appears to mitigate this.
        //
        // Unfortunately we cannot just use a single global PollEvented instance
        // either, since we can't compare Handles or assume they will always
        // point to the exact same reactor.
        let stream = globals().receiver.try_clone()?;
        let wakeup = PollEvented::new(stream)?;

        Ok(Driver { wakeup })
    }

    /// Drain all data in the global receiver, ensuring we'll get woken up when
    /// there is a write on the other end.
    ///
    /// We do *NOT* use the existence of any read bytes as evidence a signal was
    /// received since the `pending` flags would have already been set if that
    /// was the case. See
    /// [#38](https://github.com/alexcrichton/tokio-signal/issues/38) for more
    /// info.
    fn drain(&mut self, cx: &mut Context<'_>) {
        loop {
            match Pin::new(&mut self.wakeup).poll_read(cx, &mut [0; 128]) {
                Poll::Ready(Ok(0)) => panic!("EOF on self-pipe"),
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(e)) => panic!("Bad read on self-pipe: {}", e),
                Poll::Pending => break,
            }
        }
    }
}

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
pub struct Signal {
    driver: Driver,
    rx: Receiver<()>,
}

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
///   [`signal_hook::FORBIDDEN`](https://docs.rs/signal-hook/*/signal_hook/fn.register.html#panics)
pub fn signal(kind: SignalKind) -> io::Result<Signal> {
    let signal = kind.0;

    // Turn the signal delivery on once we are ready for it
    signal_enable(signal)?;

    // Ensure there's a driver for our associated event loop processing
    // signals.
    let driver = Driver::new()?;

    // One wakeup in a queue is enough, no need for us to buffer up any
    // more.
    let (tx, rx) = channel(1);
    globals().register_listener(signal as EventId, tx);

    Ok(Signal { driver, rx })
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
    /// use tokio::signal::unix::Signal;
    ///
    /// struct MyFuture {
    ///     signal: Signal,
    /// }
    ///
    /// impl Future for MyFuture {
    ///     type Output = Option<()>;
    ///
    ///     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    ///         println!("polling MyFuture");
    ///         self.signal.poll_recv(cx)
    ///     }
    /// }
    /// ```
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>> {
        let _ = self.driver.poll(cx);
        self.rx.poll_recv(cx)
    }
}

cfg_stream! {
    impl crate::stream::Stream for Signal {
        type Item = ();

        fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<()>> {
            self.poll_recv(cx)
        }
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
        signal_enable(-1).unwrap_err();
    }

    #[test]
    fn signal_enable_error_on_forbidden_input() {
        signal_enable(signal_hook_registry::FORBIDDEN[0]).unwrap_err();
    }
}
