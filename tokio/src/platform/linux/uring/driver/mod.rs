mod accept;

mod close;
pub(crate) use close::Close;

mod connect;

mod fsync;

mod op;
use mio::unix::SourceFd;
pub(crate) use op::Op;

mod open;

mod read;

mod recv_from;

mod rename_at;

mod send_to;

mod shared_fd;
pub(crate) use shared_fd::SharedFd;

mod socket;
pub(crate) use socket::Socket;

mod unlink_at;

mod util;

mod write;

use io_uring::{cqueue, IoUring};
use scoped_tls::scoped_thread_local;
use slab::Slab;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;

use crate::runtime::io::Registration;
use crate::io::Interest;
use crate::loom::sync::Mutex;

pub(crate) struct Driver {
    inner: Arc<Inner>,
}

/// A reference to an io_uring driver.
#[derive(Clone)]
pub(crate) struct Handle {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    io_uring: Mutex<InnerIoUring>,

    registration: Registration,
}

pub(crate) struct InnerIoUring {
    /// In-flight operations
    ops: Ops,

    /// IoUring bindings
    pub(crate) uring: IoUring,
}

// When dropping the driver, all in-flight operations must have completed. This
// type wraps the slab and ensures that, on drop, the slab is empty.
struct Ops(Slab<op::Lifecycle>);

self::scoped_thread_local!(pub(crate) static CURRENT: Arc<Inner>);

impl Driver {
    pub(crate) fn new(io_driver_handle: crate::runtime::io::Handle) -> io::Result<Driver> {
        let uring = IoUring::new(256)?;

        // Register io_uring with mio.
        let io_uring_fd = uring.as_raw_fd();
        let mut io = SourceFd(&io_uring_fd);
        let registration = Registration::new_with_interest_and_handle(
            &mut io,
            crate::io::Interest::READABLE,
            io_driver_handle,
        )?;

        let inner = Arc::new(Inner {
            io_uring: Mutex::new(InnerIoUring {
                ops: Ops::new(),
                uring,
            }),
            registration,
        });

        Ok(Driver { inner })
    }

    /// Enter the driver context. This enables using uring types.
    #[allow(dead_code)]
    pub(crate) fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT.set(&self.inner, f)
    }

    pub(crate) fn tick(&self) {
        self.inner.io_uring.lock().tick();
    }

    fn wait(&self) -> io::Result<usize> {
        let mut inner = self.inner.io_uring.lock();
        let inner = &mut *inner;

        inner.uring.submit_and_wait(1)
    }

    fn num_operations(&self) -> usize {
        let inner = self.inner.io_uring.lock();
        inner.ops.0.len()
    }

    pub(crate) fn handle(&self) -> Handle {
        Handle {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Handle {
    /// Enter the driver context. This enables using uring types.
    pub(crate) fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT.set(&self.inner, f)
    }

    pub(crate) fn try_flush(&self) {
        if let Some(mut inner) = self.inner.io_uring.try_lock() {
            let _ = inner.submit();
        }
    }

    pub(crate) fn flush(&self) {
        let _ = self.inner.io_uring.lock().submit();
    }

    pub(crate) fn try_tick(&self) {
        let _res = self.inner.registration.try_io(Interest::READABLE, || {
            let mut inner_gaurd = self.inner.io_uring.lock();
            inner_gaurd.tick();
            let _ = inner_gaurd.submit();
            Ok(())
        });
    }
}

impl std::fmt::Debug for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("io_uring Driver").finish()
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("io_uring Handle").finish()
    }
}

cfg_rt! {
    impl Handle {
        /// Returns a handle to the current reactor.
        ///
        /// # Panics
        ///
        /// This function panics if there is no current reactor set and `rt` feature
        /// flag is not enabled.
        #[track_caller]
        #[allow(dead_code)]
        pub(super) fn current() -> Self {
            crate::runtime::context::io_uring_handle().expect("A Tokio 1.x context was found, but IO is disabled. Call `enable_io` on the runtime builder to enable IO.")
        }
    }
}

cfg_not_rt! {
    impl Handle {
        /// Returns a handle to the current reactor.
        ///
        /// # Panics
        ///
        /// This function panics if there is no current reactor set, or if the `rt`
        /// feature flag is not enabled.
        #[track_caller]
        pub(super) fn current() -> Self {
            panic!("{}", crate::util::error::CONTEXT_MISSING_ERROR)
        }
    }
}

impl InnerIoUring {
    fn tick(&mut self) {
        let mut cq = self.uring.completion();
        cq.sync();

        for cqe in cq {
            if cqe.user_data() == u64::MAX {
                // Result of the cancellation action. There isn't anything we
                // need to do here. We must wait for the CQE for the operation
                // that was canceled.
                continue;
            }

            let index = cqe.user_data() as _;

            self.ops.complete(index, resultify(&cqe), cqe.flags());
        }
    }

    pub(crate) fn submit(&mut self) -> io::Result<()> {
        loop {
            match self.uring.submit() {
                Ok(_) => {
                    self.uring.submission().sync();
                    return Ok(());
                }
                Err(ref e) if e.raw_os_error() == Some(libc::EBUSY) => {
                    self.tick();
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }
}

impl AsRawFd for Driver {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.io_uring.lock().uring.as_raw_fd()
    }
}

impl AsRawFd for Handle {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.io_uring.lock().uring.as_raw_fd()
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        while self.num_operations() > 0 {
            // If waiting fails, ignore the error. The wait will be attempted
            // again on the next loop.
            let _ = self.wait().unwrap();
            self.tick();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.registration
            .deregister(&mut SourceFd(&self.io_uring.lock().uring.as_raw_fd()))
            .expect("Could not deregister io-Uring driver.");
    }
}

impl Ops {
    fn new() -> Ops {
        Ops(Slab::with_capacity(64))
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut op::Lifecycle> {
        self.0.get_mut(index)
    }

    // Insert a new operation
    fn insert(&mut self) -> usize {
        self.0.insert(op::Lifecycle::Submitted)
    }

    // Remove an operation
    fn remove(&mut self, index: usize) {
        self.0.remove(index);
    }

    fn complete(&mut self, index: usize, result: io::Result<u32>, flags: u32) {
        if self.0[index].complete(result, flags) {
            self.0.remove(index);
        }
    }
}

impl Drop for Ops {
    fn drop(&mut self) {
        assert!(self.0.is_empty());
    }
}

fn resultify(cqe: &cqueue::Entry) -> io::Result<u32> {
    let res = cqe.result();

    if res >= 0 {
        Ok(res as u32)
    } else {
        Err(io::Error::from_raw_os_error(-res))
    }
}
