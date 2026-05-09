#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use io_uring::IoUring;

// Currently, we are running some of the tests on Kernels where io_uring is not supported
// to check if the fallback mechanism works, this comes with the limitation that we are not
// able to run some checks (e.g., asserting a poll returns pending). This utiliy function
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
