use super::super::{Driver, Handle};
use super::RegisterResult;
use crate::runtime::driver::op::Lifecycle;

use io_uring::{opcode, Probe};
use std::io;
use std::os::fd::AsRawFd;

// Builds a driver whose io_uring context is stored and registered, or returns
// `None` when io_uring is unavailable on the host.
fn ready_handle() -> Option<(Driver, Handle)> {
    let (driver, handle) = Driver::new(1024, None).ok()?;

    let mut probe = Probe::new();
    handle.try_init(&mut probe).ok()?;
    handle.get_uring().lock().uring.as_ref()?;
    handle.uring_probe.set(Some(probe)).unwrap();

    Some((driver, handle))
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

// A failed reactor registration must not leave the ring stored. The next
// `try_init` would otherwise see a stored ring and skip registration.
#[test]
#[cfg_attr(miri, ignore)]
fn register_failure_does_not_store_the_ring() {
    let (_driver, handle) = Driver::new(1024, None).unwrap();
    let uring = match io_uring::IoUring::new(super::DEFAULT_RING_SIZE) {
        Ok(uring) => uring,
        Err(err) if unsupported_uring(&err) => return,
        Err(err) => panic!("unexpected setup error: {err}"),
    };

    // This fd is already in the reactor, so publishing it fails with EEXIST.
    // Storing the ring before that registration would leave it in place.
    let registry = handle.registry.try_clone().expect("clone reactor registry");
    super::register_uring_fd(&registry, uring.as_raw_fd()).expect("pre-register the ring fd");

    {
        let mut guard = handle.get_uring().lock();
        let err = super::publish_ring(&registry, &mut guard, uring)
            .expect_err("second registration of the same fd");
        assert_eq!(err.raw_os_error(), Some(libc::EEXIST));
        assert!(guard.uring.is_none(), "failed registration stored the ring");
    }

    let mut probe = Probe::new();
    handle
        .try_init(&mut probe)
        .expect("retry after a registration failure");
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
