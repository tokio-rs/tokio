//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

pub extern crate libc;
extern crate mio;
extern crate nix;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::os::unix::io::RawFd;
use std::collections::HashSet;
use std::io;

use self::libc::c_int;
use self::nix::sys::signal::{sigaction, SigAction, SigHandler, SigSet, SA_NOCLDSTOP, SA_RESTART};
use self::nix::sys::signal::Signal as NixSignal;
use self::nix::Error as NixError;
use self::nix::Errno;
use self::nix::sys::socket::{recv, send, socketpair, AddressFamily, SockType, SockFlag, MSG_DONTWAIT};
use self::mio::{Evented, Token, Ready, PollOpt};
use self::mio::Poll as MioPoll;
use self::mio::unix::EventedFd;
use futures::{Async, AsyncSink, Future, IntoFuture};
use futures::sync::mpsc::{Receiver, Sender, channel};
use futures::{Sink, Stream, Poll};
use tokio_core::reactor::{Handle, CoreId, PollEvented};
use tokio_core::io::IoFuture;

pub use self::libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
pub use self::libc::{SIGHUP, SIGQUIT, SIGPIPE, SIGALRM, SIGTRAP};

// Number of different unix signals
const SIGNUM: usize = 32;

#[derive(Default)]
struct SignalInfo {
    initialized: bool,
    // The ones interested in this signal
    recipients: Vec<Sender<c_int>>,
    // TODO: Other stuff, like the previous sigaction to call
}

struct Globals {
    pending: [AtomicBool; SIGNUM],
    sender: RawFd,
    receiver: RawFd,
    signals: [Mutex<SignalInfo>; SIGNUM],
    drivers: Mutex<HashSet<CoreId>>,
}

impl Globals {
    fn new() -> Self {
        // TODO: Better error handling
        // We use socket pair instead of pipe, as it allows send() and recv().
        let (receiver, sender) = socketpair(AddressFamily::Unix, SockType::Stream, 0, SockFlag::empty()).unwrap();
        Globals {
            // Bunch of false values
            pending: Default::default(),
            sender: sender,
            receiver: receiver,
            signals: Default::default(),
            drivers: Mutex::new(HashSet::new()),
        }
    }
}

lazy_static! {
    // TODO: Get rid of lazy_static once the prototype is done ‒ get rid of the dependency as well
    // as the possible lock in there, which *might* be problematic in signals
    static ref GLOBALS: Globals = Globals::new();
}

// Flag the relevant signal and wake up through a self-pipe
extern "C" fn pipe_wakeup(signal: c_int) {
    let index = signal as usize;
    // TODO: Handle the old signal handler
    // It might be good enough to use some lesser ordering than this, but how to prove it?
    GLOBALS.pending[index].store(true, Ordering::SeqCst);
    // Send a wakeup, ignore any errors (anything reasonably possible is full pipe and then it will
    // wake up anyway).
    let _ = send(GLOBALS.sender, &[0u8], MSG_DONTWAIT);
}

// Make sure we listen to the given signal and provide the recipient end of the self-pipe
fn signal_enable(signal: c_int) {
    let index = signal as usize;
    let mut siginfo = GLOBALS.signals[index].lock().unwrap();
    if !siginfo.initialized {
        let action = SigAction::new(SigHandler::Handler(pipe_wakeup), SA_NOCLDSTOP | SA_RESTART, SigSet::empty());
        unsafe { sigaction(NixSignal::from_c_int(signal).unwrap(), &action).unwrap() };
        // TODO: Handle the old signal handler
        siginfo.initialized = true;
    }
}

struct EventedReceiver;

impl Evented for EventedReceiver {
    fn register(&self, poll: &MioPoll, token: Token, events: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&GLOBALS.receiver).register(poll, token, events, opts)
    }
    fn reregister(&self, poll: &MioPoll, token: Token, events: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&GLOBALS.receiver).reregister(poll, token, events, opts)
    }
    fn deregister(&self, poll: &MioPoll) -> io::Result<()> {
        EventedFd(&GLOBALS.receiver).deregister(poll)
    }
}

// There'll be stuff inside
struct Driver {
    id: CoreId,
    wakeup: PollEvented<EventedReceiver>,
}

impl Future for Driver {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        // Drain the data from the pipe and maintain interest in getting more
        let any_wakeup = self.drain();
        if any_wakeup {
            self.broadcast();
        }
        // This task just lives until the end of the event loop
        Ok(Async::NotReady)
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let mut drivers = GLOBALS.drivers.lock().unwrap();
        drivers.remove(&self.id);
    }
}

impl Driver {
    fn new(handle: &Handle) -> Self {
        Driver {
            id: handle.id(),
            // TODO: Any chance of errors here?
            wakeup: PollEvented::new(EventedReceiver, handle).unwrap(),
        }
    }
    // Drain all data in the pipe and maintain an interest in read-ready
    fn drain(&self) -> bool {
        // Inform tokio we're interested in reading. It also hints on
        // if we may be readable.
        if let Async::NotReady = self.wakeup.poll_read() {
            return false;
        }
        // Read all available data (until EAGAIN)
        let mut received = false;
        let mut buffer = [0; 1024];
        loop {
            match recv(GLOBALS.receiver, &mut buffer, MSG_DONTWAIT) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => received = true,
                Err(NixError::Sys(Errno::EAGAIN)) => break,
                Err(NixError::Sys(Errno::EINTR)) => (),
                Err(e) => panic!("Bad read on self-pipe: {}", e),
            }
        }

        // If we got here, it's because we got EAGAIN above. Ask for more data.
        self.wakeup.need_read();

        received
    }
    // Go through all the signals and broadcast everything
    fn broadcast(&self) {
        for (sig, value) in GLOBALS.pending.iter().enumerate() {
            // Any signal of this kind arrived since we checked last?
            if value.swap(false, Ordering::SeqCst) {
                let signum = sig as c_int;
                let mut siginfo = GLOBALS.signals[sig].lock().unwrap();
                // It doesn't seem to be possible to do this through the iterators for now.
                // This trick is copied from https://github.com/rust-lang/rfcs/pull/1353.
                for i in (0 .. siginfo.recipients.len()).rev() {
                    // TODO: This thing probably generates unnecessary wakups of this task.
                    // But let's optimise it later on, when we know this works.
                    match siginfo.recipients[i].start_send(signum) {
                        // We don't care if it was full or not ‒ we just want to wake up the other
                        // side.
                        Ok(AsyncSink::Ready) => {
                            // We are required to call this if we push something inside
                            let _ = siginfo.recipients[i].poll_complete();
                        },
                        // The channel is full -> it'll get woken up anyway
                        Ok(AsyncSink::NotReady(_)) => (),
                        // The other side disappeared, drop this end.
                        Err(_) => {
                            siginfo.recipients.swap_remove(i);
                        },
                    }
                }
            }
        }
    }
}

// TODO: Go through the docs, they are a copy-paste from the previous version
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
/// * While multiple event loops are supported, the *first* event loop to
///   register a signal handler is required to be active to ensure that signals
///   for other event loops are delivered. In other words, once an event loop
///   registers a signal, it's best to keep it around and running. This is
///   normally just a problem for tests, and the "workaround" is to spawn a
///   thread in the background at the beginning of the test suite which is
///   running an event loop (and listening for a signal).
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
/// * Signal handling in general is relatively inefficient. Although some
///   improvements are possible in this crate, it's recommended to not plan on
///   having millions of signal channels open.
///
/// * Currently the "driver task" to process incoming signals never exits.
///
/// If you've got any questions about this feel free to open an issue on the
/// repo, though, as I'd love to chat about this! In other words, I'd love to
/// alleviate some of these limitations if possible!
pub struct Signal(Receiver<c_int>);

impl Signal {
    // TODO: Revisit the docs, they are from the previous version
    /// Creates a new stream which will receive notifications when the current
    /// process receives the signal `signum`.
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
    /// * While multiple event loops are supported, the first event loop to
    ///   register a signal handler must be active to deliver signal
    ///   notifications
    /// * Once a signal handle is registered with the process the underlying
    ///   libc signal handler is never unregistered.
    ///
    /// A `Signal` stream can be created for a particular signal number
    /// multiple times. When a signal is received then all the associated
    /// channels will receive the signal notification.
    pub fn new(signal: c_int, handle: &Handle) -> IoFuture<Signal> {
        let index = signal as usize;
        // One wakeup in a queue is enough
        let (sender, receiver) = channel(1);
        {
            let mut siginfo = GLOBALS.signals[index].lock().unwrap();
            siginfo.recipients.push(sender);
        }
        // Turn the signal delivery on once we are ready for it
        signal_enable(signal);
        let id = handle.id();
        {
            let mut drivers = GLOBALS.drivers.lock().unwrap();
            if !drivers.contains(&id) {
                handle.spawn(Driver::new(handle));
                drivers.insert(id);
            }
        }
        // TODO: Init the driving task for this handle
        Ok(Signal(receiver)).into_future().boxed()
    }
}

impl Stream for Signal {
    type Item = c_int;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<c_int>, io::Error> {
        // It seems the channel doesn't generate any errors anyway
        self.0.poll().map_err(|_| io::Error::new(io::ErrorKind::Other, "Unknown futures::sync::mpsc error"))
    }
}

// TODO: Drop for Signal and remove the other end proactively?
