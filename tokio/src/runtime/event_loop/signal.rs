//! The host's notification of a remote wakeup: everything that would unpark
//! a native runtime's thread (a spawn, a wake from another thread, a nearer
//! timer, an `Unparker`) also goes through here.
//!
//! The case to exclude is a wakeup whose work the host never learns of: the
//! waking thread reads a stale `busy = true` and skips the callback, while
//! the host, after `end`, reads the inject queue as empty. `next_sleep`
//! checks the queue without its lock, so the order comes from `pending`
//! instead: every write to it is a swap, so the swaps in `wake` and `end`
//! are totally ordered and one reads what the other wrote. If `end` reads
//! the `true` of a wakeup, the host sees the push made before it; if `wake`
//! reads the `false` of `end`, it sees `busy = false` and calls back.
//! `loom_current_thread::event_loop` models this.

use crate::loom::sync::atomic::AtomicBool;
use crate::runtime::Callback;

use std::io;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

#[cfg(unix)]
use std::os::fd::{AsFd, BorrowedFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

pub(crate) struct SignalCfg {
    pub(crate) on_work_available: Option<Callback>,
    #[cfg(unix)]
    pub(crate) event_fd: bool,
}

pub(crate) struct Signal {
    /// Set while the loop is being ticked or parked. The callback is held
    /// back then: the work a wakeup carries is the loop's own to find,
    /// through `next_sleep` or the driver's return.
    busy: AtomicBool,
    /// Set by the first wakeup since the last tick began or ended, so the
    /// host is notified once per period rather than once per wakeup.
    pending: AtomicBool,
    on_work_available: Option<Callback>,
    #[cfg(unix)]
    event_fd: Option<EventFd>,
}

/// A standalone wakeup descriptor for a loop without an I/O driver: like
/// the reactor's, the read end is readable from a wakeup until the next tick
/// drains it.
#[cfg(unix)]
struct EventFd {
    rx: UnixStream,
    tx: UnixStream,
}

impl Signal {
    pub(crate) fn new(cfg: SignalCfg, io_enabled: bool) -> io::Result<Signal> {
        #[cfg(not(unix))]
        let _ = io_enabled;
        Ok(Signal {
            busy: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            on_work_available: cfg.on_work_available,
            #[cfg(unix)]
            event_fd: if cfg.event_fd && !io_enabled {
                Some(EventFd::new()?)
            } else {
                None
            },
        })
    }

    pub(crate) fn wake(&self) {
        if self.pending.swap(true, AcqRel) {
            return;
        }
        #[cfg(unix)]
        if let Some(fd) = &self.event_fd {
            fd.signal();
        }
        if !self.busy.load(Acquire) {
            self.callback();
        }
    }

    /// An explicit `Unparker::unpark`: the callback always runs, busy or
    /// not, as a tick consumes it without leaving work behind.
    pub(crate) fn unpark(&self) {
        let first = !self.pending.swap(true, AcqRel);
        #[cfg(unix)]
        if let Some(fd) = &self.event_fd {
            if first {
                fd.signal();
            }
        }
        #[cfg(not(unix))]
        let _ = first;
        self.callback();
    }

    fn callback(&self) {
        if let Some(f) = &self.on_work_available {
            f();
        }
    }

    /// The loop is about to be ticked or parked: it consumes what the
    /// wakeups so far queued.
    pub(crate) fn begin(&self) {
        self.busy.store(true, Release);
        #[cfg(unix)]
        if let Some(fd) = &self.event_fd {
            fd.drain();
        }
        // A swap rather than a store so that every write to `pending` is an
        // RMW: loom only tracks a total modification order between RMWs
        // (tokio-rs/loom#282), and the model relies on that order.
        self.pending.swap(false, AcqRel);
    }

    /// The host is idle again: the next wakeup is its to be told of, even if
    /// one during the tick already made the descriptor readable. `busy` is
    /// cleared first, so a wakeup coalesced before the swap is never left
    /// holding `pending` without having seen the host idle.
    pub(crate) fn end(&self) {
        self.busy.store(false, Release);
        self.pending.swap(false, AcqRel);
    }

    /// Whether a wakeup has arrived since the last tick or park. For an
    /// `unpark` this is the only trace it leaves for `next_sleep`.
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.load(Acquire)
    }

    #[cfg(unix)]
    pub(crate) fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.event_fd.as_ref().map(|fd| fd.rx.as_fd())
    }
}

#[cfg(unix)]
impl EventFd {
    fn new() -> io::Result<EventFd> {
        let (rx, tx) = UnixStream::pair()?;
        rx.set_nonblocking(true)?;
        tx.set_nonblocking(true)?;
        Ok(EventFd { rx, tx })
    }

    fn signal(&self) {
        let _ = io::Write::write(&mut &self.tx, &[1]);
    }

    /// `pending` admits a signal per period, so few bytes ever accumulate.
    fn drain(&self) {
        let mut buf = [0u8; 8];
        while matches!(io::Read::read(&mut &self.rx, &mut buf), Ok(n) if n > 0) {}
    }
}

impl std::fmt::Debug for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal").finish()
    }
}
