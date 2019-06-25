//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

pub use libc;

use std::io::prelude::*;
use std::io::{self, Error, ErrorKind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, ONCE_INIT};

use futures::future;
use futures::sync::mpsc::{channel, Receiver};
use futures::{Async, Future};
use futures::{Poll, Stream};
use libc::c_int;
use mio_uds::UnixStream;
use tokio_io::IoFuture;
use tokio_reactor::{Handle, PollEvented};

use crate::registry::{globals, EventId, EventInfo, Globals, Init, Storage};

pub use libc::{SIGALRM, SIGHUP, SIGPIPE, SIGQUIT, SIGTRAP};
pub use libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};

/// BSD-specific definitions
#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
))]
pub mod bsd {
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    pub use super::libc::SIGINFO;
}

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

pub(crate) struct SignalInfo {
    event_info: EventInfo,
    init: Once,
    initialized: AtomicBool,
}

impl Default for SignalInfo {
    fn default() -> SignalInfo {
        SignalInfo {
            event_info: Default::default(),
            init: ONCE_INIT,
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

/// Enable this module to receive signal notifications for the `signal`
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
    // such case it is not run, registered is still `Ok(())`, initialized is still false.
    if siginfo.initialized.load(Ordering::Relaxed) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            "Failed to register signal handler",
        ))
    }
}

struct Driver {
    wakeup: PollEvented<UnixStream>,
}

impl Future for Driver {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        // Drain the data from the pipe and maintain interest in getting more
        self.drain();
        // Broadcast any signals which were received
        globals().broadcast();

        // This task just lives until the end of the event loop
        Ok(Async::NotReady)
    }
}

impl Driver {
    fn new(handle: &Handle) -> io::Result<Driver> {
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
        let wakeup = PollEvented::new_with_handle(stream, handle)?;

        Ok(Driver { wakeup: wakeup })
    }

    /// Drain all data in the global receiver, ensuring we'll get woken up when
    /// there is a write on the other end.
    ///
    /// We do *NOT* use the existence of any read bytes as evidence a sigal was
    /// received since the `pending` flags would have already been set if that
    /// was the case. See #38 for more info.
    fn drain(&mut self) {
        loop {
            match self.wakeup.read(&mut [0; 128]) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => {}
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("Bad read on self-pipe: {}", e),
            }
        }
    }
}

/// An implementation of `Stream` for receiving a particular type of signal.
///
/// This structure implements the `Stream` trait and represents notifications
/// of the current process receiving a particular signal. The signal being
/// listened for is passed to `Signal::new`, and the same signal number is then
/// yielded as each element for the stream.
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
/// * Currently the "driver task" to process incoming signals never exits. This
///   driver task runs in the background of the event loop provided, and
///   in general you shouldn't need to worry about it.
///
/// If you've got any questions about this feel free to open an issue on the
/// repo, though, as I'd love to chat about this! In other words, I'd love to
/// alleviate some of these limitations if possible!
pub struct Signal {
    driver: Driver,
    signal: c_int,
    rx: Receiver<()>,
}

impl Signal {
    /// Creates a new stream which will receive notifications when the current
    /// process receives the signal `signal`.
    ///
    /// This function will create a new stream which binds to the default event
    /// loop. This function returns a future which will
    /// then resolve to the signal stream, if successful.
    ///
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
    pub fn new(signal: c_int) -> IoFuture<Signal> {
        Signal::with_handle(signal, &Handle::default())
    }

    /// Creates a new stream which will receive notifications when the current
    /// process receives the signal `signal`.
    ///
    /// This function will create a new stream which may be based on the
    /// event loop handle provided. This function returns a future which will
    /// then resolve to the signal stream, if successful.
    ///
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
    pub fn with_handle(signal: c_int, handle: &Handle) -> IoFuture<Signal> {
        let handle = handle.clone();
        Box::new(future::lazy(move || {
            let result = (|| {
                // Turn the signal delivery on once we are ready for it
                signal_enable(signal)?;

                // Ensure there's a driver for our associated event loop processing
                // signals.
                let driver = Driver::new(&handle)?;

                // One wakeup in a queue is enough, no need for us to buffer up any
                // more. NB: channels always guarantee at least one slot per sender,
                // so we don't need additional slots
                let (tx, rx) = channel(0);
                globals().register_listener(signal as EventId, tx);

                Ok(Signal {
                    driver: driver,
                    rx: rx,
                    signal: signal,
                })
            })();
            future::result(result)
        }))
    }
}

impl Stream for Signal {
    type Item = c_int;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<c_int>, io::Error> {
        self.driver.poll().unwrap();

        self.rx
            .poll()
            .map(|ready| ready.map(|item| item.map(|()| self.signal)))
            // receivers don't generate errors
            .map_err(|_| unreachable!())
    }
}

#[cfg(test)]
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
