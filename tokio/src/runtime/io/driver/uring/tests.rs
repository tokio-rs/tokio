use super::super::{Driver, Handle};
use super::RegisterResult;
use crate::runtime::driver::op::Lifecycle;

use io_uring::{opcode, Probe};
use std::os::fd::AsRawFd;
use std::sync::atomic::Ordering;

// Builds a driver whose io_uring context is initialized, or returns `None` when
// io_uring is unavailable on the host.
fn ready_handle() -> Option<(Driver, Handle)> {
    let (driver, handle) = Driver::new(1024, None).ok()?;

    let mut probe = Probe::new();
    {
        let mut guard = handle.get_uring().lock();
        if !guard.try_init(&mut probe).ok()? {
            return None;
        }
    }
    handle.uring_probe.set(Some(probe)).unwrap();

    Some((driver, handle))
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

// A failed reactor registration must not leave the ring stored. The next
// call would otherwise treat it as already set up and skip registration.
#[test]
#[cfg_attr(miri, ignore)]
fn register_failure_drops_the_ring_so_init_can_retry() {
    let (_driver, handle) = Driver::new(1024, None).unwrap();
    super::FAIL_NEXT_ADD_URING_SOURCE.store(true, Ordering::Relaxed);

    let mut probe = Probe::new();
    let first = handle.try_init(&mut probe);
    let Err(err) = first else {
        super::FAIL_NEXT_ADD_URING_SOURCE.store(false, Ordering::Relaxed);
        panic!("expected registration to fail");
    };

    if err.raw_os_error() != Some(libc::EMFILE) {
        super::FAIL_NEXT_ADD_URING_SOURCE.store(false, Ordering::Relaxed);
        // The host has no io_uring, so init failed before registration.
        return;
    }

    assert!(handle.get_uring().lock().uring.is_none());
    // `register_probe` rejects a probe that was already filled.
    let mut retry_probe = Probe::new();
    handle
        .try_init(&mut retry_probe)
        .expect("retry after dropping the unregistered ring");
    assert!(handle.get_uring().lock().uring.is_some());
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
