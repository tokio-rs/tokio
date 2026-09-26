#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::{
    fs,
    time::{Duration, Instant},
};

use io_uring::IoUring;

// Currently, we are running some of the tests on Kernels where io_uring is not supported
// to check if the fallback mechanism works, this comes with the limitation that we are not
// able to run some checks (e.g., asserting a poll returns pending). This utility function
// is useful when we want to run a test only in Linux targets where io_uring is supported.
#[allow(dead_code)]
pub fn io_uring_supported() -> bool {
    match IoUring::new(256) {
        Ok(_) => true,
        // ENOSYS: kernel does not support io_uring.
        // EPERM: io_uring disabled via sysctl kernel.io_uring_disabled (#7691).
        Err(e)
            if e.raw_os_error() == Some(libc::ENOSYS) || e.raw_os_error() == Some(libc::EPERM) =>
        {
            false
        }
        Err(e) => unreachable!(
            "IoUring::new failed with an unexpected error (expected ENOSYS or EPERM): {e}"
        ),
    }
}

/// Whether tokio uses io_uring, rather than the `spawn_blocking` fallback, for
/// the `fs` functions built on `opcode` (`Statx` for `try_exists`, `Read` for
/// `read`). Mirrors the gate in `tokio/src/fs`: the target must have
/// `libc::statx` (see the FIXME there about musl) and the kernel must support
/// the opcode. Tests that assert on the io_uring path (e.g. that the first
/// poll is `Pending`) return early when this is false, since the blocking
/// fallback may already be done by the first poll.
#[allow(dead_code)]
pub fn uring_fs_op_in_use(opcode: u8) -> bool {
    if !cfg!(any(target_env = "gnu", target_os = "android")) {
        return false;
    }
    let Ok(ring) = IoUring::new(2) else {
        return false;
    };
    let mut probe = io_uring::Probe::new();
    ring.submitter().register_probe(&mut probe).is_ok() && probe.is_supported(opcode)
}

#[allow(dead_code)]
pub async fn assert_fds_are_not_leaking(count_before: usize, opened_files: usize, timeout: u64) {
    let fd_check_start = Instant::now();

    let max_leaked_fd = opened_files / 2;

    while fd_check_start.elapsed() < Duration::from_secs(timeout) {
        tokio::task::yield_now().await;

        let fd_count_after_cancel = fs::read_dir("/proc/self/fd").unwrap().count();
        let leaked = fd_count_after_cancel.saturating_sub(count_before);

        // Since we are opening {opened_files} files, we expect that the related fds
        // related to this operation will be closed. Since some other fds
        // can be opened in the meantime, we expect this number to be higher
        // than the counter before opening the files. This number could be
        // lower, but to avoid test flakiness we check that this is at most
        // half the number of the file we opened to check if there's a leak.
        if leaked <= max_leaked_fd {
            // test success
            return;
        }
    }
    panic!("Number of FDs is staying above {max_leaked_fd}. There is probably an FD leak.");
}
