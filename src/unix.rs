//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

pub extern crate libc;
extern crate mio;
extern crate mio_uds;

use std::cell::UnsafeCell;
use std::collections::HashSet;
use std::io::prelude::*;
use std::io;
use std::mem;
use std::os::unix::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, ONCE_INIT};

use futures::future;
use futures::sync::mpsc::{Receiver, Sender, channel};
use futures::{Async, AsyncSink, Future};
use futures::{Sink, Stream, Poll};
use self::libc::c_int;
use self::mio::Poll as MioPoll;
use self::mio::unix::EventedFd;
use self::mio::{Evented, Token, Ready, PollOpt};
use self::mio_uds::UnixStream;
use tokio_io::IoFuture;
use tokio_core::reactor::{Handle, CoreId, PollEvented};

pub use self::libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
pub use self::libc::{SIGHUP, SIGQUIT, SIGPIPE, SIGALRM, SIGTRAP};

// Number of different unix signals
const SIGNUM: usize = 32;

struct SignalInfo {
    pending: AtomicBool,
    // The ones interested in this signal
    recipients: Mutex<Vec<Box<Sender<c_int>>>>,

    init: Once,
    initialized: UnsafeCell<bool>,
    prev: UnsafeCell<libc::sigaction>,
}

struct Globals {
    sender: UnixStream,
    receiver: UnixStream,
    signals: [SignalInfo; SIGNUM],
    drivers: Mutex<HashSet<CoreId>>,
}

impl Default for SignalInfo {
    fn default() -> SignalInfo {
        SignalInfo {
            pending: AtomicBool::new(false),
            init: ONCE_INIT,
            initialized: UnsafeCell::new(false),
            recipients: Mutex::new(Vec::new()),
            prev: UnsafeCell::new(unsafe { mem::zeroed() }),
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
                signals: Default::default(),
                drivers: Mutex::new(HashSet::new()),
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
/// Those two operations shoudl both be async-signal safe. After that's done we
/// just try to call a previous signal handler, if any, to be "good denizens of
/// the internet"
extern fn handler(signum: c_int,
                  info: *mut libc::siginfo_t,
                  ptr: *mut libc::c_void) {
    type FnSigaction = extern fn(c_int, *mut libc::siginfo_t, *mut libc::c_void);
    type FnHandler = extern fn(c_int);
    unsafe {
        let slot = match (*GLOBALS).signals.get(signum as usize) {
            Some(slot) => slot,
            None => return,
        };
        slot.pending.store(true, Ordering::SeqCst);

        // Send a wakeup, ignore any errors (anything reasonably possible is
        // full pipe and then it will wake up anyway).
        drop((*GLOBALS).sender.write(&[1]));

        let fnptr = (*slot.prev.get()).sa_sigaction;
        if fnptr == 0 || fnptr == libc::SIG_DFL || fnptr == libc::SIG_IGN {
            return
        }
        if (*slot.prev.get()).sa_flags & libc::SA_SIGINFO == 0 {
            let action = mem::transmute::<usize, FnHandler>(fnptr);
            action(signum)
        } else {
            let action = mem::transmute::<usize, FnSigaction>(fnptr);
            action(signum, info, ptr)
        }
    }
}

/// Enable this module to receive signal notifications for the `signal`
/// provided.
///
/// This will register the signal handler if it hasn't already been registered,
/// returning any error along the way if that fails.
fn signal_enable(signal: c_int) -> io::Result<()> {
    let siginfo = match globals().signals.get(signal as usize) {
        Some(slot) => slot,
        None => {
            return Err(io::Error::new(io::ErrorKind::Other, "signal too large"))
        }
    };
    unsafe {
        #[cfg(target_os = "android")]
        fn flags() -> libc::c_ulong {
            (libc::SA_RESTART as libc::c_ulong) |
                libc::SA_SIGINFO |
                (libc::SA_NOCLDSTOP as libc::c_ulong)
        }
        #[cfg(not(target_os = "android"))]
        fn flags() -> c_int {
            libc::SA_RESTART |
                libc::SA_SIGINFO |
                libc::SA_NOCLDSTOP
        }
        let mut err = None;
        siginfo.init.call_once(|| {
            let mut new: libc::sigaction = mem::zeroed();
            new.sa_sigaction = handler as usize;
            new.sa_flags = flags();
            if libc::sigaction(signal, &new, &mut *siginfo.prev.get()) != 0 {
                err = Some(io::Error::last_os_error());
            } else {
                *siginfo.initialized.get() = true;
            }
        });
        if let Some(err) = err {
            return Err(err)
        }
        if *siginfo.initialized.get() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other,
                               "failed to register signal handler"))
        }
    }
}

/// A helper struct to register our global receiving end of the signal pipe on
/// multiple event loops.
///
/// This structure represents registering the receiving end on all event loops,
/// and uses `EventedFd` in mio to do so. It's stored in each driver task and is
/// used to read data and register interest in new signals coming in.
struct EventedReceiver;

impl Evented for EventedReceiver {
    fn register(&self, poll: &MioPoll, token: Token, events: Ready, opts: PollOpt) -> io::Result<()> {
        let fd = globals().receiver.as_raw_fd();
        match EventedFd(&fd).register(poll, token, events, opts) {
            Ok(()) => Ok(()),
            // Due to tokio-rs/tokio-core#307
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn reregister(&self, poll: &MioPoll, token: Token, events: Ready, opts: PollOpt) -> io::Result<()> {
        let fd = globals().receiver.as_raw_fd();
        EventedFd(&fd).reregister(poll, token, events, opts)
    }
    fn deregister(&self, poll: &MioPoll) -> io::Result<()> {
        let fd = globals().receiver.as_raw_fd();
        EventedFd(&fd).deregister(poll)
    }
}

impl Read for EventedReceiver {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&globals().receiver).read(buf)
    }
}

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
        let mut drivers = globals().drivers.lock().unwrap();
        drivers.remove(&self.id);
    }
}

impl Driver {
    fn new(handle: &Handle) -> io::Result<Driver> {
        Ok(Driver {
            id: handle.id(),
            wakeup: try!(PollEvented::new(EventedReceiver, handle)),
        })
    }

    /// Drain all data in the global receiver, returning whether data was to be
    /// had.
    ///
    /// If this function returns `true` then some signal has been received since
    /// we last checked, otherwise `false` indicates that no signal has been
    /// received.
    fn drain(&mut self) -> bool {
        let mut received = false;
        loop {
            match self.wakeup.read(&mut [0; 128]) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => received = true,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("Bad read on self-pipe: {}", e),
            }
        }
        received
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
                continue
            }

            let signum = sig as c_int;
            let mut recipients = slot.recipients.lock().unwrap();

            // Notify all waiters on this signal that the signal has been
            // received. If we can't push a message into the queue then we don't
            // worry about it as everything is coalesced anyway. If the channel
            // has gone away then we can remove that slot.
            for i in (0..recipients.len()).rev() {
                // TODO: This thing probably generates unnecessary wakups of
                //       this task when `NotReady` is received because we don't
                //       actually want to get woken up to continue sending a
                //       message. Let's optimise it later on though, as we know
                //       this works.
                match recipients[i].start_send(signum) {
                    Ok(AsyncSink::Ready) => {}
                    Ok(AsyncSink::NotReady(_)) => {}
                    Err(_) => { recipients.swap_remove(i); }
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
    signal: c_int,
    // Used only as an identifier. We place the real sender into a Box, so it
    // stays on the same address forever. That gives us a unique pointer, so we
    // can use this to identify the sender in a Vec and delete it when we are
    // dropped.
    id: *const Sender<c_int>,
    rx: Receiver<c_int>,
}

// The raw pointer prevents the compiler from determining it as Send
// automatically. But the only thing we use the raw pointer for is to identify
// the correct Box to delete, not manipulate any data through that.
unsafe impl Send for Signal {}

impl Signal {
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
    pub fn new(signal: c_int, handle: &Handle) -> IoFuture<Signal> {
        let result = (|| {
            // Turn the signal delivery on once we are ready for it
            try!(signal_enable(signal));

            // Ensure there's a driver for our associated event loop processing
            // signals.
            let id = handle.id();
            let mut drivers = globals().drivers.lock().unwrap();
            if !drivers.contains(&id) {
                handle.spawn(try!(Driver::new(handle)));
                drivers.insert(id);
            }
            drop(drivers);

            // One wakeup in a queue is enough, no need for us to buffer up any
            // more.
            let (tx, rx) = channel(1);
            let tx = Box::new(tx);
            let id: *const _ = &*tx;
            let idx = signal as usize;
            globals().signals[idx].recipients.lock().unwrap().push(tx);
            Ok(Signal {
                rx: rx,
                id: id,
                signal: signal,
            })
        })();

        Box::new(future::result(result))
    }
}

impl Stream for Signal {
    type Item = c_int;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<c_int>, io::Error> {
        // receivers don't generate errors
        self.rx.poll().map_err(|_| panic!())
    }
}

impl Drop for Signal {
    fn drop(&mut self) {
        let idx = self.signal as usize;
        let mut list = globals().signals[idx].recipients.lock().unwrap();
        list.retain(|sender| &**sender as *const _ != self.id);
    }
}
