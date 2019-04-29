//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

pub extern crate libc;
extern crate mio;
extern crate mio_uds;
extern crate signal_hook_registry;

use std::io::prelude::*;
use std::io::{self, Error, ErrorKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, ONCE_INIT};

use self::libc::c_int;
use self::mio_uds::UnixStream;
use futures::future;
use futures::sync::mpsc::{channel, Receiver, Sender};
use futures::{Async, Future};
use futures::{Poll, Stream};
use tokio_io::IoFuture;
use tokio_reactor::{Handle, PollEvented};

pub use self::libc::{SIGALRM, SIGHUP, SIGPIPE, SIGQUIT, SIGTRAP};
pub use self::libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};

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

// Number of different unix signals
// (FreeBSD has 33)
const SIGNUM: usize = 33;

type SignalSender = Sender<c_int>;

struct SignalInfo {
    pending: AtomicBool,
    // The ones interested in this signal
    recipients: Mutex<Vec<Box<SignalSender>>>,

    init: Once,
    initialized: AtomicBool,
}

struct Globals {
    sender: UnixStream,
    receiver: UnixStream,
    signals: Vec<SignalInfo>,
}

impl Globals {
    /// Register a new `Signal` instance's channel sender.
    /// Returns a `SignalId` which should be later used for deregistering
    /// this sender.
    fn register_signal_sender(signal: c_int, tx: SignalSender) -> SignalId {
        let tx = Box::new(tx);
        let id = SignalId::from(&tx);

        let idx = signal as usize;
        globals().signals[idx].recipients.lock().unwrap().push(tx);
        id
    }

    /// Deregister a `Signal` instance's channel sender because the `Signal`
    /// is no longer interested in receiving events (e.g. dropped).
    fn deregister_signal_receiver(signal: c_int, id: SignalId) {
        let idx = signal as usize;
        let mut list = globals().signals[idx].recipients.lock().unwrap();
        list.retain(|sender| SignalId::from(sender) != id);
    }
}

/// A newtype which represents a unique identifier for each `Signal` instance.
/// The id is derived by boxing the channel `Sender` associated with this instance
/// and using its address in memory.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SignalId(usize);

impl<'a> From<&'a Box<SignalSender>> for SignalId {
    fn from(tx: &'a Box<SignalSender>) -> Self {
        SignalId(&**tx as *const _ as usize)
    }
}

impl Default for SignalInfo {
    fn default() -> SignalInfo {
        SignalInfo {
            pending: AtomicBool::new(false),
            init: ONCE_INIT,
            initialized: AtomicBool::new(false),
            recipients: Mutex::new(Vec::new()),
        }
    }
}

static mut GLOBALS: *mut Globals = 0 as *mut Globals;

fn globals() -> &'static Globals {
    static INIT: Once = ONCE_INIT;

    unsafe {
        INIT.call_once(|| {
            let (receiver, sender) = UnixStream::pair().unwrap();
            let globals = Globals {
                sender: sender,
                receiver: receiver,
                signals: (0..SIGNUM).map(|_| Default::default()).collect(),
            };
            GLOBALS = Box::into_raw(Box::new(globals));
        });
        &*GLOBALS
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
fn action(slot: &SignalInfo, mut sender: &UnixStream) {
    slot.pending.store(true, Ordering::SeqCst);

    // Send a wakeup, ignore any errors (anything reasonably possible is
    // full pipe and then it will wake up anyway).
    drop(sender.write(&[1]));
}

/// Enable this module to receive signal notifications for the `signal`
/// provided.
///
/// This will register the signal handler if it hasn't already been registered,
/// returning any error along the way if that fails.
fn signal_enable(signal: c_int) -> io::Result<()> {
    if signal_hook_registry::FORBIDDEN.contains(&signal) {
        return Err(Error::new(
            ErrorKind::Other,
            format!("Refusing to register signal {}", signal),
        ));
    }

    let globals = globals();
    let siginfo = match globals.signals.get(signal as usize) {
        Some(slot) => slot,
        None => return Err(io::Error::new(io::ErrorKind::Other, "signal too large")),
    };
    let mut registered = Ok(());
    siginfo.init.call_once(|| {
        registered = unsafe {
            signal_hook_registry::register(signal, move || action(siginfo, &globals.sender))
                .map(|_| ())
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
        self.broadcast();

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

    /// Go through all the signals and broadcast everything.
    ///
    /// Driver tasks wake up for *any* signal and simply process all globally
    /// registered signal streams, so each task is sort of cooperatively working
    /// for all the rest as well.
    fn broadcast(&self) {
        for (sig, slot) in globals().signals.iter().enumerate() {
            // Any signal of this kind arrived since we checked last?
            if !slot.pending.swap(false, Ordering::SeqCst) {
                continue;
            }

            let signum = sig as c_int;
            let mut recipients = slot.recipients.lock().unwrap();

            // Notify all waiters on this signal that the signal has been
            // received. If we can't push a message into the queue then we don't
            // worry about it as everything is coalesced anyway. If the channel
            // has gone away then we can remove that slot.
            for i in (0..recipients.len()).rev() {
                match recipients[i].try_send(signum) {
                    Ok(()) => {}
                    Err(ref e) if e.is_disconnected() => {
                        recipients.swap_remove(i);
                    }

                    // Channel is full, ignore the error since the
                    // receiver has already been woken up
                    Err(e) => {
                        // Sanity check in case this error type ever gets
                        // additional variants we have not considered.
                        debug_assert!(e.is_full());
                    }
                }
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
    id: SignalId,
    rx: Receiver<c_int>,
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
                let id = Globals::register_signal_sender(signal, tx);
                Ok(Signal {
                    driver: driver,
                    rx: rx,
                    id: id,
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
        // receivers don't generate errors
        self.rx.poll().map_err(|_| panic!())
    }
}

impl Drop for Signal {
    fn drop(&mut self) {
        Globals::deregister_signal_receiver(self.signal, self.id);
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio;

    use super::*;

    #[test]
    fn dropped_signal_senders_are_cleaned_up() {
        let mut rt =
            self::tokio::runtime::current_thread::Runtime::new().expect("failed to init runtime");

        let signum = libc::SIGUSR1;
        let signal = rt
            .block_on(Signal::new(signum))
            .expect("failed to create signal");

        {
            let recipients = globals().signals[signum as usize]
                .recipients
                .lock()
                .unwrap();
            assert!(!recipients.is_empty());
        }

        drop(signal);

        unsafe {
            assert_eq!(libc::kill(libc::getpid(), signum), 0);
        }

        {
            let recipients = globals().signals[signum as usize]
                .recipients
                .lock()
                .unwrap();
            assert!(recipients.is_empty());
        }
    }
}
