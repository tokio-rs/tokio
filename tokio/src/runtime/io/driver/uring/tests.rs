use super::super::{Driver, Handle};
use super::{RegisterResult, UringContext};
use crate::runtime::driver::op::Lifecycle;
use crate::runtime::Builder;

use io_uring::{opcode, Probe};
use std::os::fd::AsRawFd;

// Builds a driver whose io_uring context is stored and registered, or returns
// `None` when io_uring can't be set up on the host.
fn ready_handle() -> Option<(Driver, Handle)> {
    let (driver, mut handle) = Driver::new(1024, None).ok()?;
    handle.setup_uring().ok()?;
    Some((driver, handle))
}

// Returns `true` if `UringContext` can set up io_uring on this host.
fn host_supports_uring() -> bool {
    ready_handle().is_some()
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

// A failed reactor registration is returned as a setup error (build
// fails), and no half set up ring or probe is stored.
#[test]
#[cfg_attr(miri, ignore)]
fn register_failure_does_not_store_the_ring() {
    // Skip on hosts without io_uring: setup would fail before reaching the
    // registration step.
    if !host_supports_uring() {
        return;
    }

    let (_driver, mut handle) = Driver::new(1024, None).unwrap();
    let _broken = BrokenRegistry::new(&handle);

    // `try_init` fails at the registration and doesn't store the ring.
    let mut context = UringContext::new();
    let Err(err) = context.try_init(&handle.registry, &mut Probe::new()) else {
        panic!("registration against /dev/null should fail");
    };
    // ENOSYS/EPERM would mean setup failed before the registration, so this
    // wouldn't be testing a registration failure.
    assert!(
        !matches!(err.raw_os_error(), Some(libc::ENOSYS | libc::EPERM)),
        "unexpected registration error: {err}"
    );
    assert!(
        context.uring.is_none(),
        "failed registration stored the ring"
    );

    assert!(
        UringContext::setup(&handle.registry).is_err(),
        "setup should return the registration error"
    );

    handle
        .setup_uring()
        .expect_err("setup_uring should return the registration error");
    assert!(
        handle.get_uring().lock().uring.is_none(),
        "setup_uring stored a ring after a failed registration"
    );
    assert!(
        handle.uring_probe.is_none(),
        "setup_uring stored a probe after a failed registration"
    );
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

// `setup_uring` sets up the ring and probe before any operation is submitted.
#[test]
#[cfg_attr(miri, ignore)]
fn driver_with_uring_sets_up_ring() {
    let Some((_driver, handle)) = ready_handle() else {
        return;
    };

    assert!(handle.get_uring().lock().uring.is_some());
    assert!(handle.is_uring_ready(opcode::Read::CODE));
}

// Without `setup_uring`, the driver doesn't create a ring.
#[test]
#[cfg_attr(miri, ignore)]
fn driver_without_uring_has_no_ring() {
    let (_driver, handle) = Driver::new(1024, None).unwrap();

    assert!(handle.uring_probe.is_none());
    assert!(handle.get_uring().lock().uring.is_none());
    assert!(!handle.is_uring_ready(opcode::Read::CODE));
}

// A runtime built with only `enable_io` doesn't set up io_uring.
#[test]
#[cfg_attr(miri, ignore)]
fn runtime_with_enable_io_has_no_uring() {
    let rt = Builder::new_current_thread().enable_io().build().unwrap();

    rt.block_on(async {
        assert!(!super::uring_supports(opcode::Read::CODE));
    });
}

// Checks a runtime built with io_uring requested: it uses io_uring when the
// host supports it, and fails to build otherwise.
fn assert_runtime_requires_uring(builder: &mut Builder) {
    let result = builder.build();

    if !host_supports_uring() {
        assert!(
            result.is_err(),
            "build() should fail when io_uring can't be set up"
        );
        return;
    }

    let rt = result.expect("build() failed although io_uring is available");
    rt.block_on(async {
        assert!(super::uring_supports(opcode::Read::CODE));
    });
}

// A runtime built with `enable_io_uring` uses io_uring, or fails to build if
// io_uring can't be set up.
#[test]
#[cfg_attr(miri, ignore)]
fn runtime_with_enable_io_uring_requires_uring() {
    assert_runtime_requires_uring(Builder::new_current_thread().enable_io_uring());
}

// `enable_all` includes `enable_io_uring` when io_uring is compiled in, so it
// behaves the same.
#[test]
#[cfg_attr(miri, ignore)]
fn runtime_with_enable_all_requires_uring() {
    assert_runtime_requires_uring(Builder::new_current_thread().enable_all());
}
