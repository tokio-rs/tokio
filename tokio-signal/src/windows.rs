//! Windows-specific types for signal handling.
//!
//! This module is only defined on Windows and contains the primary `Event` type
//! for receiving notifications of events. These events are listened for via the
//! `SetConsoleCtrlHandler` function which receives events of the type
//! `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`

#![cfg(windows)]

extern crate mio;
extern crate winapi;

use std::cell::RefCell;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, ONCE_INIT};

use self::winapi::shared::minwindef::*;
use self::winapi::um::wincon::*;
use futures::future;
use futures::stream::Fuse;
use futures::sync::mpsc;
use futures::sync::oneshot;
use futures::{Async, Future, IntoFuture, Poll, Stream};
use mio::Ready;
use tokio_reactor::{Handle, PollEvented};

use IoFuture;

extern "system" {
    fn SetConsoleCtrlHandler(HandlerRoutine: usize, Add: BOOL) -> BOOL;
}

static INIT: Once = ONCE_INIT;
static mut GLOBAL_STATE: *mut GlobalState = 0 as *mut _;

/// Stream of events discovered via `SetConsoleCtrlHandler`.
///
/// This structure can be used to listen for events of the type `CTRL_C_EVENT`
/// and `CTRL_BREAK_EVENT`. The `Stream` trait is implemented for this struct
/// and will resolve for each notification received by the process. Note that
/// there are few limitations with this as well:
///
/// * A notification to this process notifies *all* `Event` streams for that
///   event type.
/// * Notifications to an `Event` stream **are coalesced** if they aren't
///   processed quickly enough. This means that if two notifications are
///   received back-to-back, then the stream may only receive one item about the
///   two notifications.
pub struct Event {
    reg: PollEvented<MyRegistration>,
    _finished: oneshot::Sender<()>,
}

struct GlobalState {
    ready: mio::SetReadiness,
    tx: mpsc::UnboundedSender<Message>,
    ctrl_c: GlobalEventState,
    ctrl_break: GlobalEventState,
}

struct GlobalEventState {
    ready: AtomicBool,
}

enum Message {
    NewEvent(DWORD, oneshot::Sender<io::Result<Event>>),
}

struct DriverTask {
    handle: Handle,
    reg: PollEvented<MyRegistration>,
    rx: Fuse<mpsc::UnboundedReceiver<Message>>,
    ctrl_c: EventState,
    ctrl_break: EventState,
}

struct EventState {
    tasks: Vec<(RefCell<oneshot::Receiver<()>>, mio::SetReadiness)>,
}

impl Event {
    /// Creates a new stream listening for the `CTRL_C_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_c() -> IoFuture<Event> {
        Event::ctrl_c_handle(&Handle::current())
    }

    /// Creates a new stream listening for the `CTRL_C_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_c_handle(handle: &Handle) -> IoFuture<Event> {
        Event::new(CTRL_C_EVENT, handle)
    }

    /// Creates a new stream listening for the `CTRL_BREAK_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_break() -> IoFuture<Event> {
        Event::ctrl_break_handle(&Handle::current())
    }

    /// Creates a new stream listening for the `CTRL_BREAK_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_break_handle(handle: &Handle) -> IoFuture<Event> {
        Event::new(CTRL_BREAK_EVENT, handle)
    }

    fn new(signum: DWORD, handle: &Handle) -> IoFuture<Event> {
        let mut init = None;
        INIT.call_once(|| {
            init = Some(global_init(handle));
        });
        let new_signal = future::lazy(move || {
            let (tx, rx) = oneshot::channel();
            let msg = Message::NewEvent(signum, tx);
            let res = unsafe { (*GLOBAL_STATE).tx.clone().unbounded_send(msg) };
            res.expect(
                "failed to request a new signal stream, did the \
                 first event loop go away?",
            );
            rx.then(|r| r.unwrap())
        });
        match init {
            Some(init) => Box::new(init.into_future().and_then(|()| new_signal)),
            None => Box::new(new_signal),
        }
    }
}

impl Stream for Event {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<()>, io::Error> {
        if !self.reg.poll_read_ready(Ready::readable())?.is_ready() {
            return Ok(Async::NotReady);
        }
        self.reg.clear_read_ready(Ready::readable())?;
        self.reg
            .get_ref()
            .inner
            .borrow()
            .as_ref()
            .unwrap()
            .1
            .set_readiness(mio::Ready::empty())
            .expect("failed to set readiness");
        Ok(Async::Ready(Some(())))
    }
}

fn global_init(handle: &Handle) -> io::Result<()> {
    let (tx, rx) = mpsc::unbounded();
    let reg = MyRegistration {
        inner: RefCell::new(None),
    };
    let reg = try!(PollEvented::new_with_handle(reg, handle));
    let ready = reg.get_ref().inner.borrow().as_ref().unwrap().1.clone();
    unsafe {
        let state = Box::new(GlobalState {
            ready: ready,
            ctrl_c: GlobalEventState {
                ready: AtomicBool::new(false),
            },
            ctrl_break: GlobalEventState {
                ready: AtomicBool::new(false),
            },
            tx: tx,
        });
        GLOBAL_STATE = Box::into_raw(state);

        let rc = SetConsoleCtrlHandler(handler as usize, TRUE);
        if rc == 0 {
            Box::from_raw(GLOBAL_STATE);
            GLOBAL_STATE = 0 as *mut _;
            return Err(io::Error::last_os_error());
        }

        ::tokio_executor::spawn(Box::new(DriverTask {
            handle: handle.clone(),
            rx: rx.fuse(),
            reg: reg,
            ctrl_c: EventState { tasks: Vec::new() },
            ctrl_break: EventState { tasks: Vec::new() },
        }));

        Ok(())
    }
}

impl Future for DriverTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.check_event_drops();
        self.check_messages();
        self.check_events().unwrap();

        // TODO: when to finish this task?
        Ok(Async::NotReady)
    }
}

impl DriverTask {
    fn check_event_drops(&mut self) {
        self.ctrl_c
            .tasks
            .retain(|task| !task.0.borrow_mut().poll().is_err());
        self.ctrl_break
            .tasks
            .retain(|task| !task.0.borrow_mut().poll().is_err());
    }

    fn check_messages(&mut self) {
        loop {
            // Acquire the next message
            let message = match self.rx.poll().unwrap() {
                Async::Ready(Some(e)) => e,
                Async::Ready(None) | Async::NotReady => break,
            };
            let (sig, complete) = match message {
                Message::NewEvent(sig, complete) => (sig, complete),
            };

            let event = if sig == CTRL_C_EVENT {
                &mut self.ctrl_c
            } else {
                &mut self.ctrl_break
            };

            // Acquire the (registration, set_readiness) pair by... assuming
            // we're on the event loop (true because of the spawn above).
            let reg = MyRegistration {
                inner: RefCell::new(None),
            };
            let reg = match PollEvented::new_with_handle(reg, &self.handle) {
                Ok(reg) => reg,
                Err(e) => {
                    drop(complete.send(Err(e)));
                    continue;
                }
            };

            // Create the `Event` to pass back and then also keep a handle to
            // the `SetReadiness` for ourselves internally.
            let (tx, rx) = oneshot::channel();
            let ready = reg.get_ref().inner.borrow_mut().as_mut().unwrap().1.clone();
            drop(complete.send(Ok(Event {
                reg: reg,
                _finished: tx,
            })));
            event.tasks.push((RefCell::new(rx), ready));
        }
    }

    fn check_events(&mut self) -> io::Result<()> {
        if self.reg.poll_read_ready(Ready::readable())?.is_not_ready() {
            return Ok(());
        }
        self.reg.clear_read_ready(Ready::readable())?;
        self.reg
            .get_ref()
            .inner
            .borrow()
            .as_ref()
            .unwrap()
            .1
            .set_readiness(mio::Ready::empty())
            .unwrap();

        if unsafe { (*GLOBAL_STATE).ctrl_c.ready.swap(false, Ordering::SeqCst) } {
            for task in self.ctrl_c.tasks.iter() {
                task.1.set_readiness(mio::Ready::readable()).unwrap();
            }
        }
        if unsafe {
            (*GLOBAL_STATE)
                .ctrl_break
                .ready
                .swap(false, Ordering::SeqCst)
        } {
            for task in self.ctrl_break.tasks.iter() {
                task.1.set_readiness(mio::Ready::readable()).unwrap();
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn handler(ty: DWORD) -> BOOL {
    let event = match ty {
        CTRL_C_EVENT => &(*GLOBAL_STATE).ctrl_c,
        CTRL_BREAK_EVENT => &(*GLOBAL_STATE).ctrl_break,
        _ => return FALSE,
    };
    if event.ready.swap(true, Ordering::SeqCst) {
        FALSE
    } else {
        drop((*GLOBAL_STATE).ready.set_readiness(mio::Ready::readable()));
        // TODO: this will report that we handled a CTRL_BREAK_EVENT when in
        //       fact we may not have any streams actually created for that
        //       event.
        TRUE
    }
}

struct MyRegistration {
    inner: RefCell<Option<(mio::Registration, mio::SetReadiness)>>,
}

impl mio::Evented for MyRegistration {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        events: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        let reg = mio::Registration::new2();
        reg.0.register(poll, token, events, opts)?;
        *self.inner.borrow_mut() = Some(reg);
        Ok(())
    }

    fn reregister(
        &self,
        _poll: &mio::Poll,
        _token: mio::Token,
        _events: mio::Ready,
        _opts: mio::PollOpt,
    ) -> io::Result<()> {
        Ok(())
    }

    fn deregister(&self, _poll: &mio::Poll) -> io::Result<()> {
        Ok(())
    }
}
