//! Windows-specific types for signal handling.
//!
//! This module is only defined on Windows and contains the primary `Event` type
//! for receiving notifications of events. These events are listened for via the
//! `SetConsoleCtrlHandler` function which receives events of the type
//! `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`

#![cfg(windows)]

use super::registry::{globals, EventId, EventInfo, Init, Storage};

use tokio_sync::mpsc::{channel, Receiver};

use futures_core::stream::Stream;
use std::convert::TryFrom;
use std::io;
use std::pin::Pin;
use std::sync::Once;
use std::task::{Context, Poll};
use winapi::shared::minwindef::*;
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::wincon::*;

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
pub(crate) struct OsExtraData {}

impl Init for OsExtraData {
    fn init() -> Self {
        Self {}
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

impl Event {
    fn new(signum: DWORD) -> io::Result<Self> {
        global_init()?;

        let (tx, rx) = channel(1);
        globals().register_listener(signum as EventId, tx);

        Ok(Event { rx })
    }
}

pub(crate) fn ctrl_c() -> io::Result<Event> {
    Event::new(CTRL_C_EVENT)
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

unsafe extern "system" fn handler(ty: DWORD) -> BOOL {
    let globals = globals();
    globals.record_event(ty as EventId);

    // According to https://docs.microsoft.com/en-us/windows/console/handlerroutine
    // the handler routine is always invoked in a new thread, thus we don't
    // have the same restrictions as in Unix signal handlers, meaning we can
    // go ahead and perform the broadcast here.
    if globals.broadcast() {
        TRUE
    } else {
        // No one is listening for this notification any more
        // let the OS fire the next (possibly the default) handler.
        FALSE
    }
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

/// Creates a new stream which receives "ctrl-break" notifications sent to the
/// process.
///
/// This function binds to the default reactor.
pub fn ctrl_break() -> io::Result<CtrlBreak> {
    Event::new(CTRL_BREAK_EVENT).map(|inner| CtrlBreak { inner })
}

impl Stream for CtrlBreak {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|item| item.map(|_| ()))
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::runtime::current_thread::Runtime;

    use futures_util::stream::StreamExt;

    #[test]
    fn ctrl_c() {
        let mut rt = Runtime::new().unwrap();

        rt.block_on(async {
            let ctrl_c = crate::signal::ctrl_c().expect("failed to create CtrlC");

            // Windows doesn't have a good programmatic way of sending events
            // like sending signals on Unix, so we'll stub out the actual OS
            // integration and test that our handling works.
            unsafe {
                super::handler(CTRL_C_EVENT);
            }

            let _ = ctrl_c.into_future().await;
        });
    }

    #[test]
    fn ctrl_break() {
        let mut rt = Runtime::new().unwrap();

        rt.block_on(async {
            let ctrl_break = super::ctrl_break().expect("failed to create CtrlC");

            // Windows doesn't have a good programmatic way of sending events
            // like sending signals on Unix, so we'll stub out the actual OS
            // integration and test that our handling works.
            unsafe {
                super::handler(CTRL_BREAK_EVENT);
            }

            let _ = ctrl_break.into_future().await;
        });
    }
}
