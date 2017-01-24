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
use tokio_core::io::IoFuture;
use tokio_core::reactor::{Handle, CoreId, PollEvented};

pub use self::libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
pub use self::libc::{SIGHUP, SIGQUIT, SIGPIPE, SIGALRM, SIGTRAP};

// Number of different unix signals
const SIGNUM: usize = 32;

struct SignalInfo {
    pending: AtomicBool,
    // The ones interested in this signal
    recipients: Mutex<Vec<Sender<c_int>>>,

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

// Flag the relevant signal and wake up through a self-pipe
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

// Make sure we listen to the given signal and provide the recipient end of the
// self-pipe
fn signal_enable(signal: c_int) -> io::Result<()> {
    let siginfo = &globals().signals[signal as usize];
	unsafe {
        let mut err = None;
        siginfo.init.call_once(|| {
            let mut new: libc::sigaction = mem::zeroed();
            new.sa_sigaction = handler as usize;
            new.sa_flags = libc::SA_RESTART |
                            libc::SA_SIGINFO |
                            libc::SA_NOCLDSTOP;
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

struct EventedReceiver;

impl Evented for EventedReceiver {
    fn register(&self, poll: &MioPoll, token: Token, events: Ready, opts: PollOpt) -> io::Result<()> {
        let fd = globals().receiver.as_raw_fd();
        EventedFd(&fd).register(poll, token, events, opts)
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

    // Drain all data in the pipe and maintain an interest in read-ready
    fn drain(&self) -> bool {
        // Inform tokio we're interested in reading. It also hints on
        // if we may be readable.
        if let Async::NotReady = self.wakeup.poll_read() {
            return false;
        }
        // Read all available data (until EAGAIN)
        let mut received = false;
        loop {
            match (&globals().receiver).read(&mut [0; 128]) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => received = true,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("Bad read on self-pipe: {}", e),
            }
        }

        // If we got here, it's because we got EAGAIN above. Ask for more data.
        self.wakeup.need_read();

        received
    }
    // Go through all the signals and broadcast everything
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
            // worry about it as everything is coalesced anyway.
            for i in (0..recipients.len()).rev() {
                // TODO: This thing probably generates unnecessary wakups of
                //       this task. But let's optimise it later on, when we
                //       know this works.
                match recipients[i].start_send(signum) {
                    Ok(AsyncSink::Ready) => {}
                    Ok(AsyncSink::NotReady(_)) => {}
                    Err(_) => { recipients.swap_remove(i); }
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
            globals().signals[signal as usize].recipients.lock().unwrap().push(tx);
            Ok(Signal(rx))
        })();

        future::result(result).boxed()
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
