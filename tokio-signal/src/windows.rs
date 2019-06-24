//! Windows-specific types for signal handling.
//!
//! This module is only defined on Windows and contains the primary `Event` type
//! for receiving notifications of events. These events are listened for via the
//! `SetConsoleCtrlHandler` function which receives events of the type
//! `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`

#![cfg(windows)]

use std::convert::TryFrom;
use std::io;
use std::sync::{Once, ONCE_INIT};

use futures::future;
use futures::sync::mpsc::{channel, Receiver, Sender};
use futures::{Async, Future, Poll, Stream};
use tokio_reactor::Handle;
use winapi::shared::minwindef::*;
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::wincon::*;

use crate::registry::{globals, EventId, EventInfo, Init, Storage};
use crate::IoFuture;

#[derive(Debug)]
pub(crate) struct OsStorage {
    ctrl_c: EventInfo,
    ctrl_break: EventInfo,
}

impl Init for OsStorage {
    fn init() -> Self {
        Self {
            ctrl_c: EventInfo::default(),
            ctrl_break: EventInfo::default(),
        }
    }
}

impl Storage for OsStorage {
    fn event_info(&self, id: EventId) -> Option<&EventInfo> {
        match DWORD::try_from(id) {
            Ok(CTRL_C_EVENT) => Some(&self.ctrl_c),
            Ok(CTRL_BREAK_EVENT) => Some(&self.ctrl_break),
            _ => None,
        }
    }

    fn for_each<'a, F>(&'a self, mut f: F)
    where
        F: FnMut(&'a EventInfo),
    {
        f(&self.ctrl_c);
        f(&self.ctrl_break);
    }
}

#[derive(Debug)]
pub(crate) struct OsExtraData {
    driver_waker: Sender<()>,
}

impl Init for OsExtraData {
    fn init() -> Self {
        let (driver_waker, driver_rx) = channel(0);

        ::tokio_executor::spawn(DriverTask { rx: driver_rx });

        Self { driver_waker }
    }
}

static INIT: Once = ONCE_INIT;

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
// FIXME: refactor and combine with unix::Signal
pub struct Event {
    rx: Receiver<()>,
}

#[derive(Debug)]
struct DriverTask {
    rx: Receiver<()>,
}

impl Event {
    /// Creates a new stream listening for the `CTRL_C_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_c() -> IoFuture<Event> {
        Event::ctrl_c_handle(&Handle::default())
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
        Event::ctrl_break_handle(&Handle::default())
    }

    /// Creates a new stream listening for the `CTRL_BREAK_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    pub fn ctrl_break_handle(handle: &Handle) -> IoFuture<Event> {
        Event::new(CTRL_BREAK_EVENT, handle)
    }

    fn new(signum: DWORD, _handle: &Handle) -> IoFuture<Event> {
        let new_signal = future::poll_fn(move || {
            let mut init = None;
            INIT.call_once(|| {
                init = Some(global_init());
            });

            if let Some(Err(e)) = init {
                return Err(e);
            }

            let (tx, rx) = channel(0);
            globals().register_listener(signum as EventId, tx);

            Ok(Async::Ready(Event { rx }))
        });

        Box::new(new_signal)
    }
}

impl Stream for Event {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<()>, io::Error> {
        self.rx
            .poll()
            // receivers don't generate errors
            .map_err(|_| unreachable!())
    }
}

fn global_init() -> io::Result<()> {
    unsafe {
        let rc = SetConsoleCtrlHandler(Some(handler), TRUE);
        if rc == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

impl Future for DriverTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            // Ensure we keep polling our waker until we know there are no more
            // events (and therefore we've registered interest to be woken again).
            match self.rx.poll() {
                Ok(Async::Ready(Some(()))) => continue,
                Ok(Async::Ready(None)) => panic!("driver got disconnected?"),
                Ok(Async::NotReady) => break,
                // receivers don't generate errors
                Err(()) => unreachable!(),
            }
        }

        globals().broadcast();

        // TODO(1000): when to finish this task?
        Ok(Async::NotReady)
    }
}

unsafe extern "system" fn handler(ty: DWORD) -> BOOL {
    let globals = globals();
    globals.record_event(ty as EventId);

    // FIXME: revisit this, we'd probably want to panic if the driver task goes away,
    // but that would unwind across the FFI boundary...
    let _ = globals.driver_waker.clone().try_send(());

    // TODO(1000): this will report that we handled a CTRL_BREAK_EVENT when
    //       in fact we may not have any streams actually created for that
    //       event.
    TRUE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::runtime::current_thread;
    use tokio::timer::Timeout;

    fn with_timeout<F: Future>(future: F) -> impl Future<Item = F::Item, Error = F::Error> {
        Timeout::new(future, Duration::from_secs(1)).map_err(|e| {
            if e.is_timer() {
                panic!("failed to register timer");
            } else if e.is_elapsed() {
                panic!("timed out")
            } else {
                e.into_inner().expect("missing inner error")
            }
        })
    }

    #[test]
    fn ctrl_c_and_ctrl_break() {
        // FIXME(1000): combining into one test due to a restriction where the
        // first event loop cannot go away
        let mut rt = current_thread::Runtime::new().unwrap();
        let event_ctrl_c = rt
            .block_on(with_timeout(Event::ctrl_c()))
            .expect("failed to run future");

        // Windows doesn't have a good programmatic way of sending events
        // like sending signals on Unix, so we'll stub out the actual OS
        // integration and test that our handling works.
        unsafe {
            super::handler(CTRL_C_EVENT);
        }

        rt.block_on(with_timeout(event_ctrl_c.into_future()))
            .ok()
            .expect("failed to run event");

        let event_ctrl_break = rt
            .block_on(with_timeout(Event::ctrl_break()))
            .expect("failed to run future");
        unsafe {
            super::handler(CTRL_BREAK_EVENT);
        }

        rt.block_on(with_timeout(event_ctrl_break.into_future()))
            .ok()
            .expect("failed to run event");
    }
}
