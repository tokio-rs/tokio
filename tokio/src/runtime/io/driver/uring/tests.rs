use super::super::{Driver, Handle};
use super::RegisterResult;
use crate::runtime::driver::op::Lifecycle;

use io_uring::{opcode, Probe};
use std::io;
use std::os::fd::AsRawFd;

// Builds a driver whose io_uring context is stored and registered, or returns
// `None` when io_uring is unavailable on the host.
fn ready_handle() -> Option<(Driver, Handle)> {
    let (driver, mut handle) = Driver::new(1024, None).ok()?;
    handle.enable_io_uring().ok()?;
    handle.uring_probe.is_some().then_some((driver, handle))
}

fn unsupported_uring(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(libc::ENOSYS) | Some(libc::EPERM))
}

// Makes the next submission fail with a non-`EBUSY` error by pointing the ring's
// file descriptor at `/dev/null`, then restores it so the driver can shut down.
struct BrokenRing {
    ring_fd: i32,
    saved: i32,
}

impl BrokenRing {
    fn new(handle: &Handle) -> Self {
        let ring_fd = handle.get_uring().lock().ring().as_raw_fd();
        let null = std::fs::File::open("/dev/null").unwrap();
        // SAFETY: `dup` and `dup2` are valid for any open file descriptor.
        let saved = unsafe { libc::dup(ring_fd) };
        assert!(saved >= 0);
        let rc = unsafe { libc::dup2(null.as_raw_fd(), ring_fd) };
        assert_eq!(rc, ring_fd);
        Self { ring_fd, saved }
    }
}

impl Drop for BrokenRing {
    fn drop(&mut self) {
        // SAFETY: `saved` is a live descriptor duplicated from the ring.
        unsafe {
            libc::dup2(self.saved, self.ring_fd);
            libc::close(self.saved);
        }
    }
}

// Points the reactor registration fd at `/dev/null` for the duration of a
// `try_init`, then puts the original epoll fd back.
struct BrokenRegistry {
    ep: i32,
    saved: i32,
}

impl BrokenRegistry {
    fn new(handle: &Handle) -> Self {
        let ep = handle.registry.as_raw_fd();
        // SAFETY: `dup` and `dup2` are valid for this open epoll descriptor.
        let saved = unsafe { libc::dup(ep) };
        assert!(saved >= 0);
        let null = std::fs::File::open("/dev/null").unwrap();
        let rc = unsafe { libc::dup2(null.as_raw_fd(), ep) };
        assert_eq!(rc, ep);
        Self { ep, saved }
    }
}

impl Drop for BrokenRegistry {
    fn drop(&mut self) {
        // SAFETY: `saved` is the epoll descriptor duplicated above.
        unsafe {
            libc::dup2(self.saved, self.ep);
            libc::close(self.saved);
        }
    }
}

// A failed reactor registration must not leave the ring stored. The next
// `try_init` would otherwise see a stored ring and skip registration.
// A later call fills a new probe from that ring and does not replace it.
#[test]
#[cfg_attr(miri, ignore)]
fn register_failure_does_not_store_the_ring() {
    let (_driver, handle) = Driver::new(1024, None).unwrap();
    match io_uring::IoUring::new(super::DEFAULT_RING_SIZE) {
        Ok(_) => {}
        Err(err) if unsupported_uring(&err) => return,
        Err(err) => panic!("unexpected setup error: {err}"),
    }

    {
        let _broken = BrokenRegistry::new(&handle);
        let mut probe = Probe::new();
        let err = handle
            .try_init(&mut probe)
            .expect_err("registration against /dev/null");
        assert_ne!(err.raw_os_error(), Some(libc::ENOSYS));
        assert!(
            handle.get_uring().lock().uring.is_none(),
            "failed registration stored the ring"
        );
    }

    let mut probe = Probe::new();
    handle
        .try_init(&mut probe)
        .expect("retry after a registration failure");
    let fd = handle.get_uring().lock().ring().as_raw_fd();

    let mut probe = Probe::new();
    handle.try_init(&mut probe).expect("existing ring");
    assert!(
        probe.is_supported(opcode::Nop::CODE),
        "stored ring did not fill the probe"
    );
    assert_eq!(handle.get_uring().lock().ring().as_raw_fd(), fd);
}

// Once an entry is pushed onto the submission queue the kernel may already own
// the buffer it points at, so a failing submission must keep the slot instead of
// dropping the op and freeing the buffer while it is still in use.
#[test]
#[cfg_attr(miri, ignore)]
fn register_op_keeps_entry_on_submit_failure() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let broken = BrokenRing::new(&handle);

    let entry = opcode::Nop::new().build();
    // SAFETY: the no-op entry does not reference any buffer.
    let res = unsafe { handle.register_op(entry, futures::task::noop_waker()) };

    // The entry was pushed before the submit failed, so the slot is kept and its
    // index handed back for the caller to cancel.
    let RegisterResult::Pushed { index, .. } = res else {
        panic!("expected Pushed after a post-push submit failure");
    };
    assert!(matches!(
        handle.get_uring().lock().ops.get(index),
        Some(Lifecycle::Waiting(_))
    ));

    drop(broken);
}

// `enable_io_uring` sets up the ring and probe before any operation is
// submitted.
#[test]
#[cfg_attr(miri, ignore)]
fn enable_io_uring_sets_up_ring() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    assert!(handle.get_uring().lock().uring.is_some());
    assert!(handle.is_uring_ready(opcode::Read::CODE));
}

// Without `enable_io_uring`, the driver doesn't create a ring.
#[test]
#[cfg_attr(miri, ignore)]
fn driver_has_no_ring_until_enabled() {
    let (_driver, handle) = Driver::new(1024, None).unwrap();

    assert!(handle.uring_probe.is_none());
    assert!(handle.get_uring().lock().uring.is_none());
    assert!(!handle.is_uring_ready(opcode::Read::CODE));
}
