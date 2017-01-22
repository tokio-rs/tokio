//! Unix-specific types for signal handling.
//!
//! This module is only defined on Unix platforms and contains the primary
//! `Signal` type for receiving notifications of signals.

#![cfg(unix)]

pub extern crate libc;
extern crate mio;
extern crate tokio_uds;
extern crate nix;

use std::sync::{Once, ONCE_INIT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::os::unix::io::RawFd;

use self::libc::c_int;
use self::nix::unistd::pipe;
use self::nix::sys::signal::{SigAction, SigHandler, SigSet, SA_NOCLDSTOP, SA_RESTART, sigaction};
use self::nix::sys::signal::Signal as NixSignal;
use self::nix::sys::socket::{send, MSG_DONTWAIT};

// Number of different unix signals
const SIGNUM: usize = 32;

#[derive(Default)]
struct SignalInfo {
    initialized: bool,
    // TODO: Other stuff, like the previous sigaction to call
}

struct Globals {
    pending: [AtomicBool; SIGNUM],
    sender: RawFd,
    receiver: RawFd,
    signals: [Mutex<SignalInfo>; SIGNUM],
}

impl Globals {
    fn new() -> Self {
        // TODO: Better error handling
        let (receiver, sender) = pipe().unwrap();
        Globals {
            // Bunch of false values
            pending: Default::default(),
            sender: sender,
            receiver: receiver,
            signals: Default::default(),
        }
    }
}

lazy_static! {
    // TODO: Get rid of lazy_static once the prototype is done ‒ get rid of the dependency as well
    // as the possible lock in there, which *might* be problematic in signals
    static ref globals: Globals = Globals::new();
}

// Flag the relevant signal and wake up through a self-pipe
extern "C" fn pipe_wakeup(signal: c_int) {
    let index = signal as usize;
    // TODO: Handle the old signal handler
    // It might be good enough to use some lesser ordering than this, but how to prove it?
    globals.pending[index].store(true, Ordering::SeqCst);
    // Send a wakeup, ignore any errors (anything reasonably possible is full pipe and then it will
    // wake up anyway).
    let _ = send(globals.sender, &[0u8], MSG_DONTWAIT);
}

// Make sure we listen to the given signal and provide the recipient end of the self-pipe
fn signal_enable(signal: c_int) -> RawFd {
    let index = signal as usize;
    let mut siginfo = globals.signals[index].lock().unwrap();
    if !siginfo.initialized {
        let action = SigAction::new(SigHandler::Handler(pipe_wakeup), SA_NOCLDSTOP | SA_RESTART, SigSet::empty());
        unsafe { sigaction(NixSignal::from_c_int(signal).unwrap(), &action).unwrap() };
        // TODO: Handle the old signal handler
        siginfo.initialized = true;
    }
    globals.receiver
}

use std::cell::RefCell;
use std::io::{self, Write, Read};
use std::mem;

use futures::future;
use futures::stream::Fuse;
use futures::sync::mpsc;
use futures::sync::oneshot;
use futures::{Future, Stream, IntoFuture, Poll, Async};
use self::tokio_uds::UnixStream;
use tokio_core::io::IoFuture;
use tokio_core::reactor::{PollEvented, Handle};

static INIT: Once = ONCE_INIT;
static mut GLOBAL_STATE: *mut GlobalState = 0 as *mut _;

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
pub struct Signal {
    signum: c_int,
    reg: PollEvented<MyRegistration>,
    _finished: oneshot::Sender<()>,
}

struct GlobalState {
    write: UnixStream,
    tx: mpsc::UnboundedSender<Message>,
    signals: [GlobalSignalState; 32],
}

struct GlobalSignalState {
    ready: AtomicBool,
    prev: libc::sigaction,
}

enum Message {
    NewSignal(c_int, oneshot::Sender<io::Result<Signal>>),
}

struct DriverTask {
    handle: Handle,
    read: UnixStream,
    rx: Fuse<mpsc::UnboundedReceiver<Message>>,
    signals: [SignalState; 32],
}

struct SignalState {
    registered: bool,
    tasks: Vec<(RefCell<oneshot::Receiver<()>>, mio::SetReadiness)>,
}

pub use self::libc::{SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
pub use self::libc::{SIGHUP, SIGQUIT, SIGPIPE, SIGALRM, SIGTRAP};

impl Signal {
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
    pub fn new(signum: c_int, handle: &Handle) -> IoFuture<Signal> {
        let mut init = None;
        INIT.call_once(|| {
            init = Some(global_init(handle));
        });
        let new_signal = future::lazy(move || {
            let (tx, rx) = oneshot::channel();
            let msg = Message::NewSignal(signum, tx);
            let res = unsafe {
                (*GLOBAL_STATE).tx.clone().send(msg)
            };
            res.expect("failed to request a new signal stream, did the \
                        first event loop go away?");
            rx.then(|r| r.unwrap())
        });
        match init {
            Some(init) => init.into_future().and_then(|()| new_signal).boxed(),
            None => new_signal.boxed(),
        }
    }
}

impl Stream for Signal {
    type Item = c_int;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<c_int>, io::Error> {
        if !self.reg.poll_read().is_ready() {
            return Ok(Async::NotReady)
        }
        self.reg.need_read();
        self.reg.get_ref()
                .inner.borrow()
                .as_ref().unwrap().1
                .set_readiness(mio::Ready::none())
                .expect("failed to set readiness");
        Ok(Async::Ready(Some(self.signum)))
    }
}

fn global_init(handle: &Handle) -> io::Result<()> {
    let (tx, rx) = mpsc::unbounded();
    let (read, write) = try!(UnixStream::pair(handle));
    unsafe {
        let state = Box::new(GlobalState {
            write: write,
            signals: {
                fn new() -> GlobalSignalState {
                    GlobalSignalState {
                        ready: AtomicBool::new(false),
                        prev: unsafe { mem::zeroed() },
                    }
                }
                [
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                ]
            },
            tx: tx,
        });
        GLOBAL_STATE = Box::into_raw(state);

        handle.spawn(DriverTask {
            handle: handle.clone(),
            rx: rx.fuse(),
            read: read,
            signals: {
                fn new() -> SignalState {
                    SignalState { registered: false, tasks: Vec::new() }
                }
                [
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                    new(), new(), new(), new(), new(), new(), new(), new(),
                ]
            },
        });

        Ok(())
    }
}

impl Future for DriverTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.check_signal_drops();
        self.check_messages();
        self.check_signals();

        // TODO: when to finish this task?
        Ok(Async::NotReady)
    }
}

impl DriverTask {
    fn check_signal_drops(&mut self) {
        for signal in self.signals.iter_mut() {
            signal.tasks.retain(|task| {
                !task.0.borrow_mut().poll().is_err()
            });
        }
    }

    fn check_messages(&mut self) {
        loop {
            // Acquire the next message
            let message = match self.rx.poll().unwrap() {
                Async::Ready(Some(e)) => e,
                Async::Ready(None) |
                Async::NotReady => break,
            };
            let (sig, complete) = match message {
                Message::NewSignal(sig, complete) => (sig, complete),
            };

            // If the signal's too large, then we return an error, otherwise we
            // use this index to look at the signal slot.
            //
            // If the signal wasn't previously registered then we do so now.
            let signal = match self.signals.get_mut(sig as usize) {
                Some(signal) => signal,
                None => {
                    complete.complete(Err(io::Error::new(io::ErrorKind::Other,
                                                         "signum too large")));
                    continue
                }
            };
            if !signal.registered {
                unsafe {
                    let mut new: libc::sigaction = mem::zeroed();
                    new.sa_sigaction = handler as usize;
                    new.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;
                    let mut prev = mem::zeroed();
                    if libc::sigaction(sig, &new, &mut prev) != 0 {
                        complete.complete(Err(io::Error::last_os_error()));
                        continue
                    }
                    signal.registered = true;
                }
            }

            // Acquire the (registration, set_readiness) pair by... assuming
            // we're on the event loop (true because of the spawn above).
            let reg = MyRegistration { inner: RefCell::new(None) };
            let reg = match PollEvented::new(reg, &self.handle) {
                Ok(reg) => reg,
                Err(e) => {
                    complete.complete(Err(e));
                    continue
                }
            };

            // Create the `Signal` to pass back and then also keep a handle to
            // the `SetReadiness` for ourselves internally.
            let (tx, rx) = oneshot::channel();
            let ready = reg.get_ref().inner.borrow_mut().as_mut().unwrap().1.clone();
            complete.complete(Ok(Signal {
                signum: sig,
                reg: reg,
                _finished: tx,
            }));
            signal.tasks.push((RefCell::new(rx), ready));
        }
    }

    fn check_signals(&mut self) {
        // Drain all data from the pipe
        let mut buf = [0; 32];
        let mut any = false;
        loop {
            match self.read.read(&mut buf) {
                Ok(0) => {  // EOF == something happened
                    any = true;
                    break
                }
                Ok(..) => any = true,   // data read, but keep draining
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("bad read: {}", e),
            }
        }

        // If nothing happened, no need to check the signals
        if !any {
            return
        }

        for (i, slot) in self.signals.iter().enumerate() {
            // No need to go farther if we haven't even registered a signal
            if !slot.registered {
                continue
            }

            // See if this signal actually happened since we last checked
            unsafe {
                if !(*GLOBAL_STATE).signals[i].ready.swap(false, Ordering::SeqCst) {
                    continue
                }
            }

            // Wake up all the tasks waiting on this signal
            for task in slot.tasks.iter() {
                task.1.set_readiness(mio::Ready::readable())
                      .expect("failed to set readiness");
            }
        }
    }
}

extern fn handler(signum: c_int,
                  info: *mut libc::siginfo_t,
                  ptr: *mut libc::c_void) {
    type FnSigaction = extern fn(c_int, *mut libc::siginfo_t, *mut libc::c_void);
    type FnHandler = extern fn(c_int);

    unsafe {
        let state = match (*GLOBAL_STATE).signals.get(signum as usize) {
            Some(state) => state,
            None => return,
        };

        if !state.ready.swap(true, Ordering::SeqCst) {
            // Ignore errors here as we're not in a context that can panic,
            // and otherwise there's not much we can do.
            drop((&(*GLOBAL_STATE).write).write(&[1]));
        }

        let fnptr = state.prev.sa_sigaction;
        if fnptr == 0 || fnptr == libc::SIG_DFL || fnptr == libc::SIG_IGN {
            return
        }
        if state.prev.sa_flags & libc::SA_SIGINFO == 0 {
            let action = mem::transmute::<usize, FnHandler>(fnptr);
            action(signum)
        } else {
            let action = mem::transmute::<usize, FnSigaction>(fnptr);
            action(signum, info, ptr)
        }
    }
}

struct MyRegistration {
    inner: RefCell<Option<(mio::Registration, mio::SetReadiness)>>,
}

impl mio::Evented for MyRegistration {
    fn register(&self,
                poll: &mio::Poll,
                token: mio::Token,
                events: mio::Ready,
                opts: mio::PollOpt) -> io::Result<()> {
        let reg = mio::Registration::new(poll, token, events, opts);
        *self.inner.borrow_mut() = Some(reg);
        Ok(())
    }

    fn reregister(&self,
                  _poll: &mio::Poll,
                  _token: mio::Token,
                  _events: mio::Ready,
                  _opts: mio::PollOpt) -> io::Result<()> {
        Ok(())
    }

    fn deregister(&self, _poll: &mio::Poll) -> io::Result<()> {
        Ok(())
    }
}
