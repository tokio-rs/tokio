use super::super::{Driver, Handle};
use super::RegisterResult;
use crate::runtime::driver::op::Lifecycle;

use io_uring::{opcode, Probe};
use std::io;
use std::os::fd::AsRawFd;
use std::task::Waker;

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

fn nops(count: usize) -> Vec<io_uring::squeue::Entry> {
    (0..count).map(|_| opcode::Nop::new().build()).collect()
}

// A link only reaches the next entry in the submission queue, so a chain that
// doesn't fit in the queue is refused before anything is pushed.
#[test]
#[cfg_attr(miri, ignore)]
fn register_ops_refuses_chain_larger_than_queue() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let capacity = handle.get_uring().lock().ring_mut().submission().capacity();
    // SAFETY: no-op entries do not reference any buffer.
    let res = unsafe { handle.register_ops(&nops(capacity + 1), true, Waker::noop()) };

    let RegisterResult::NotPushed(err) = res else {
        panic!("expected NotPushed for a chain larger than the queue");
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert!(handle.get_uring().lock().ops.is_empty());
}

// A batch is pushed as far as there is room, so it can be larger than the queue.
#[test]
#[cfg_attr(miri, ignore)]
fn register_ops_splits_batch_larger_than_queue() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let capacity = handle.get_uring().lock().ring_mut().submission().capacity();
    let count = capacity + capacity / 2;
    // SAFETY: no-op entries do not reference any buffer.
    let res = unsafe { handle.register_ops(&nops(count), false, Waker::noop()) };

    let RegisterResult::Submitted(index) = res else {
        panic!("expected Submitted for a batch larger than the queue");
    };
    assert_eq!(index.len(), count);
    let ctx = handle.get_uring().lock();
    assert!(index.iter().all(|&i| ctx.ops.contains(i)));
}

// When the submit fails after the entries were pushed, the slots of the pushed
// entries are kept, so the caller keeps their memory alive until they complete.
#[test]
#[cfg_attr(miri, ignore)]
fn register_ops_keeps_pushed_entries_on_submit_failure() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let broken = BrokenRing::new(&handle);

    // SAFETY: no-op entries do not reference any buffer.
    let res = unsafe { handle.register_ops(&nops(3), true, Waker::noop()) };

    let RegisterResult::Pushed { index, .. } = res else {
        panic!("expected Pushed after a post-push submit failure");
    };
    assert_eq!(index.len(), 3);
    let ctx = handle.get_uring().lock();
    for i in index {
        assert!(matches!(ctx.ops.get(i), Some(Lifecycle::Waiting(_))));
    }
    drop(ctx);

    drop(broken);
}

// A batch larger than the queue fills it, and then the flush fails. Only the
// entries that were pushed keep a slot.
#[test]
#[cfg_attr(miri, ignore)]
fn register_ops_keeps_only_pushed_entries_when_flush_fails() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let capacity = handle.get_uring().lock().ring_mut().submission().capacity();
    let broken = BrokenRing::new(&handle);

    // SAFETY: no-op entries do not reference any buffer.
    let res = unsafe { handle.register_ops(&nops(capacity + 1), false, Waker::noop()) };

    let RegisterResult::Pushed { index, .. } = res else {
        panic!("expected Pushed when the flush fails");
    };
    assert_eq!(index.len(), capacity);
    assert_eq!(handle.get_uring().lock().ops.len(), capacity);

    drop(broken);
}
