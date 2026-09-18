//! The event loop's timer: the soonest timer deadline, armed after each drive
//! as readiness on the reactor's descriptor, so the host needs no timer of
//! its own and the runtime can re-arm or cancel without involving it.

use crate::runtime::Handle;

use std::io;
use std::time::Duration;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) use timerfd::Deadline;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub(super) use unsupported::Deadline;

/// Whether the time driver is enabled, so a deadline sink is required.
#[cfg(feature = "time")]
fn needs_timer(handle: &Handle) -> bool {
    handle.inner.driver().time.is_some()
}

#[cfg(not(feature = "time"))]
fn needs_timer(_handle: &Handle) -> bool {
    false
}

/// A one-shot `timerfd` registered in the reactor's `epoll` set.
#[cfg(any(target_os = "linux", target_os = "android"))]
mod timerfd {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

    pub(crate) struct Deadline {
        fd: Option<OwnedFd>,
        armed: AtomicBool,
    }

    impl Deadline {
        pub(crate) fn new(handle: &Handle) -> io::Result<Deadline> {
            if !needs_timer(handle) {
                return Ok(Deadline {
                    fd: None,
                    armed: AtomicBool::new(false),
                });
            }
            // SAFETY: plain syscall; the returned descriptor is owned here.
            let raw = unsafe {
                libc::timerfd_create(
                    libc::CLOCK_MONOTONIC,
                    libc::TFD_NONBLOCK | libc::TFD_CLOEXEC,
                )
            };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `raw` is a freshly created, unowned descriptor.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            handle
                .inner
                .driver()
                .io()
                .register_event_loop_timer(fd.as_raw_fd())?;
            Ok(Deadline {
                fd: Some(fd),
                armed: AtomicBool::new(false),
            })
        }

        /// Arm for `after`, or disarm on `None`. Always re-programmed: a
        /// `timerfd_settime` also clears a pending expiration, so an
        /// unchanged deadline that has just fired is armed again rather than
        /// left consumed.
        pub(crate) fn arm(&self, after: Option<Duration>) {
            let Some(fd) = &self.fd else {
                debug_assert!(after.is_none(), "deadline without a time driver");
                return;
            };
            if after.is_none() && !self.armed.swap(false, Relaxed) {
                return;
            }
            self.armed.store(after.is_some(), Relaxed);
            let after = after.unwrap_or(Duration::ZERO);
            let spec = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                it_value: libc::timespec {
                    tv_sec: after.as_secs() as libc::time_t,
                    tv_nsec: after.subsec_nanos() as _,
                },
            };
            // SAFETY: `fd` is a live timerfd; `spec` is fully initialized.
            let rc =
                unsafe { libc::timerfd_settime(fd.as_raw_fd(), 0, &spec, std::ptr::null_mut()) };
            assert_eq!(rc, 0, "timerfd_settime: {}", io::Error::last_os_error());
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod unsupported {
    use super::*;

    pub(crate) struct Deadline;

    impl Deadline {
        pub(crate) fn new(handle: &Handle) -> io::Result<Deadline> {
            if needs_timer(handle) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "an event loop with the time driver needs a timer descriptor, \
                     which this platform's reactor does not provide",
                ));
            }
            Ok(Deadline)
        }

        pub(crate) fn arm(&self, after: Option<Duration>) {
            debug_assert!(after.is_none(), "deadline without a time driver");
        }
    }
}
