use super::super::{Driver, Handle};

use io_uring::{opcode, Probe};
use std::os::fd::AsRawFd;

// Builds a driver whose io_uring context is initialized, or returns `None` when
// io_uring is unavailable on the host.
fn ready_handle() -> Option<(Driver, Handle)> {
    let (driver, handle) = Driver::new(1024).ok()?;

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

// Once an entry is pushed onto the submission queue the kernel may already own
// the buffer it points at, so a failing submission must not drop the operation:
// doing so would let the `Op` free the buffer while it is still in use.
#[test]
fn register_op_keeps_entry_on_submit_failure() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let broken = BrokenRing::new(&handle);

    let entry = opcode::Nop::new().build();
    // SAFETY: the no-op entry does not reference any buffer.
    let res = unsafe { handle.register_op(entry, futures::task::noop_waker()) };

    assert!(res.is_err());
    assert!(!handle.get_uring().lock().ops.is_empty());

    drop(broken);
}
