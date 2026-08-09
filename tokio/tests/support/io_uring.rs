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
pub fn io_uring_supported() -> bool {
    match IoUring::new(256) {
        Ok(_) => true,
        // The Kernel does not support io-uring
        Err(e) if e.raw_os_error() == Some(libc::ENOSYS) => false,
        Err(_) => unreachable!(
            "The target should either support io_uring or return ENOSYS if not supported"
        ),
    }
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
