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
    let res = unsafe { handle.register_op(entry, futures::task::noop_waker(), |_| {}) };

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

// If a completion waker panics after the new SQE is pushed, the caller must
// already have been told the slab index so `Op::drop` can cancel the op
// instead of freeing its buffer while the kernel still holds it.
#[cfg(panic = "unwind")]
#[test]
#[cfg_attr(miri, ignore)]
fn register_op_records_index_before_completion_waker_panic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    struct PanicWaker;

    impl Wake for PanicWaker {
        fn wake(self: Arc<Self>) {
            panic!("completion waker panic");
        }

        fn wake_by_ref(self: &Arc<Self>) {
            panic!("completion waker panic");
        }
    }

    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    let panic_waker = Waker::from(Arc::new(PanicWaker));
    for i in 0..512 {
        let w = if i == 0 {
            panic_waker.clone()
        } else {
            futures::task::noop_waker()
        };
        // SAFETY: NOP entries do not reference a buffer.
        unsafe { handle.register_op(opcode::Nop::new().build(), w, |_| {}) };
    }

    let recorded = Arc::new(AtomicUsize::new(usize::MAX));
    let recorded_cb = recorded.clone();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        handle.register_op(
            opcode::Nop::new().build(),
            futures::task::noop_waker(),
            |idx| {
                recorded_cb.store(idx, Ordering::SeqCst);
            },
        )
    }));

    let idx = recorded.load(Ordering::SeqCst);
    if res.is_err() {
        assert_ne!(
            idx,
            usize::MAX,
            "slab index must be recorded before a completion waker panics"
        );
        assert!(
            matches!(
                handle.get_uring().lock().ops.get(idx),
                Some(Lifecycle::Waiting(_))
            ),
            "in-flight slot was dropped after the waker panic"
        );
    } else {
        assert_ne!(idx, usize::MAX, "after_push should run on the success path");
    }
}
