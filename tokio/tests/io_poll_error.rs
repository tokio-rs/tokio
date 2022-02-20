#![cfg(target_os = "linux")]

use tokio::io::{unix::AsyncFd, Interest};
use userfaultfd::{Uffd, UffdBuilder};

fn get_blocking_userfault_fd() -> Uffd {
    UffdBuilder::new()
        // yes-blocking
        .non_blocking(false)
        .create()
        .unwrap()
}

#[tokio::test]
async fn test_poll_error_propagates_to_readable() {
    // GIVEN: a blocking `userfault` fd. The only reason we use a `userfault` fd
    // is that it is a reliable way of getting `EPOLLERR`.

    // From userfault(2)
    // > If the `O_NONBLOCK` flag is not enabled, then poll(2) (always)
    // indicates the file as having a POLLERR condition, and select(2) indicates
    // the file descriptor as both readable and writable.
    let async_fd = AsyncFd::with_interest(get_blocking_userfault_fd(), Interest::READABLE).unwrap();

    // WHEN
    let result = async_fd.readable().await;

    // THEN
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Other);
}

#[tokio::test]
async fn test_poll_error_propagates_to_poll_read_ready() {
    // GIVEN: a blocking `userfault` fd. The only reason we use a `userfault` fd
    // is that it is a reliable way of getting `EPOLLERR`.

    // From userfault(2)
    // > If the `O_NONBLOCK` flag is not enabled, then poll(2) (always)
    // indicates the file as having a POLLERR condition, and select(2) indicates
    // the file descriptor as both readable and writable.
    let async_fd = AsyncFd::with_interest(get_blocking_userfault_fd(), Interest::READABLE).unwrap();

    // WHEN
    let result = futures::future::poll_fn(|cx| async_fd.poll_read_ready(cx)).await;

    // THEN
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Other);
}
