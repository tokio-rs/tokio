//! Windows-specific types for signal handling.
//!
//! This module is only defined on Windows and contains the primary `Event` type
//! for receiving notifications of events. These events are listened for via the
//! `SetConsoleCtrlHandler` function which receives events of the type
//! `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`

#![cfg(windows)]

use std::convert::TryFrom;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Once;
use std::task::{Context, Poll};

use futures_core::stream::Stream;
use tokio_reactor::Handle;
use tokio_sync::mpsc::{channel, Receiver, Sender};
use winapi::shared::minwindef::*;
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::wincon::*;

use crate::registry::{globals, EventId, EventInfo, Init, Storage};

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
        let (driver_waker, driver_rx) = channel(1);

        tokio_executor::spawn(DriverTask { rx: driver_rx });

        Self { driver_waker }
    }
}

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
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub(crate) struct Event {
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
    pub(crate) fn ctrl_c(handle: &Handle) -> io::Result<Self> {
        Event::new(CTRL_C_EVENT, handle)
    }

    /// Creates a new stream listening for the `CTRL_BREAK_EVENT` events.
    ///
    /// This function will register a handler via `SetConsoleCtrlHandler` and
    /// deliver notifications to the returned stream.
    fn ctrl_break_handle(handle: &Handle) -> io::Result<Self> {
        Event::new(CTRL_BREAK_EVENT, handle)
    }

    fn new(signum: DWORD, _handle: &Handle) -> io::Result<Self> {
        global_init()?;

        let (tx, rx) = channel(1);
        globals().register_listener(signum as EventId, tx);

        Ok(Event { rx })
    }
}

impl Stream for Event {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

fn global_init() -> io::Result<()> {
    static INIT: Once = Once::new();

    let mut init = None;
    INIT.call_once(|| unsafe {
        let rc = SetConsoleCtrlHandler(Some(handler), TRUE);
        let ret = if rc == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        };

        init = Some(ret);
    });

    init.unwrap_or_else(|| Ok(()))
}

impl Future for DriverTask {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            // Ensure we keep polling our waker until we know there are no more
            // events (and therefore we've registered interest to be woken again).
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(())) => continue,
                Poll::Ready(None) => panic!("driver got disconnected?"),
                Poll::Pending => break,
            }
        }

        globals().broadcast();

        // TODO(1000): when to finish this task?
        Poll::Pending
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

/// Represents a stream which receives "ctrl-break" notifications sent to the process
/// via `SetConsoleCtrlHandler`.
///
/// A notification to this process notifies *all* streams listening to
/// this event. Moreover, the notifications **are coalesced** if they aren't processed
/// quickly enough. This means that if two notifications are received back-to-back,
/// then the stream may only receive one item about the two notifications.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct CtrlBreak {
    inner: Event,
}

impl CtrlBreak {
    /// Creates a new stream which receives "ctrl-break" notifications sent to the
    /// process.
    ///
    /// This function binds to the default reactor.
    pub fn new() -> io::Result<Self> {
        Self::with_handle(&Handle::default())
    }

    /// Creates a new stream which receives "ctrl-break" notifications sent to the
    /// process.
    ///
    /// This function binds to reactor specified by `handle`.
    pub fn with_handle(handle: &Handle) -> io::Result<Self> {
        Event::ctrl_break_handle(handle).map(|inner| Self { inner })
    }
}

impl Stream for CtrlBreak {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|item| item.map(|_| ()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::{self, FutureExt};
    use futures_util::stream::StreamExt;
    use std::time::Duration;
    use tokio::runtime::current_thread;
    use tokio_timer::Timeout;

    fn with_timeout<F: Future>(future: F) -> impl Future<Output = F::Output> {
        Timeout::new(future, Duration::from_secs(1)).map(|result| result.expect("timed out"))
    }

    #[test]
    fn ctrl_c_and_ctrl_break() {
        // FIXME(1000): combining into one test due to a restriction where the
        // first event loop cannot go away
        let mut rt = current_thread::Runtime::new().unwrap();
        let event_ctrl_c = rt
            .block_on(with_timeout(future::lazy(|_| crate::CtrlC::new())))
            .expect("failed to run future");

        // Windows doesn't have a good programmatic way of sending events
        // like sending signals on Unix, so we'll stub out the actual OS
        // integration and test that our handling works.
        unsafe {
            super::handler(CTRL_C_EVENT);
        }

        let _ = rt.block_on(with_timeout(event_ctrl_c.into_future()));

        let event_ctrl_break = rt
            .block_on(with_timeout(future::lazy(|_| CtrlBreak::new())))
            .expect("failed to run future");

        unsafe {
            super::handler(CTRL_BREAK_EVENT);
        }

        let _ = rt.block_on(with_timeout(event_ctrl_break.into_future()));
    }
}
