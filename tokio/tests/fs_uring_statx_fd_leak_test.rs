#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

mod support {
    pub(crate) mod io_uring;
}

use std::fmt::Debug;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::fs::read;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::time::timeout;
use tokio_test::assert_pending;

use crate::support::io_uring::{assert_fds_are_not_leaking, io_uring_supported};

/// Count currently-open fds in this process.
fn fd_count() -> usize {
    fs::read_dir("/proc/self/fd").unwrap().count()
}

/// First poll:
/// - polls the inner `tokio::fs::read()` future once,
/// - expects `Pending` so we know we took the io_uring path,
/// - registers the task waker with Tokio's uring machinery.
///
/// Second poll:
/// - happens after the kernel completes the open and Tokio stores the CQE as
///   `Lifecycle::Completed(cqe)` and wakes the task,
/// - polls inner future again to execute `Op::file_metadata`/Statx operation
///
/// Third poll:
/// - happens after the kernel completes the statx and Tokio stores the CQE as
///   `Lifecycle::Completed(cqe)` and wakes the task,
/// - stays pending forever so the task can be aborted.
///
/// Aborting the task here drops the inner `Op::file_metadata()` future while Tokio
/// still has a completed CQE sitting in the slab.
struct PollOpenOnceThenNeverRepoll<F> {
    inner: Pin<Box<F>>,
    poll_senders: Vec<UnboundedSender<()>>,
    poll_pending_counts: usize,
}

impl<F: Future> Future for PollOpenOnceThenNeverRepoll<F>
where
    F::Output: Debug,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let poll_count = self.poll_pending_counts;
        let sender_count = self.poll_senders.len();
        if poll_count < sender_count {
            // The first two poll should return `Poll::Pending`` because it verifies
            //  that io_uring is enabled and then executes the open operation in
            // `tokio::fs::read`
            if poll_count != sender_count - 1 {
                assert_pending!(self.inner.as_mut().poll(cx));
            }
            self.poll_pending_counts += 1;
            self.poll_senders[poll_count].send(()).unwrap();
        }

        Poll::Pending
    }
}

async fn completed_then_dropped_before_repoll(path: PathBuf) {
    // `tokio::fs::read` has an Open operation occurring (via OpenOptions) before doing a
    // Statx operation (via `Op::file_metadata`), we must poll and complete the Open operation
    // first to then be able to poll the Statx operation.
    const POLL_PENDING_COUNT: usize = 3;
    let mut poll_senders: Vec<UnboundedSender<()>> = Vec::new();
    let mut poll_receivers = Vec::new();

    for _ in 0..POLL_PENDING_COUNT {
        let (sender, receiver) = unbounded_channel();
        poll_senders.push(sender);
        poll_receivers.push(receiver);
    }

    let handle = tokio::spawn(async move {
        let fut = read(&path);
        PollOpenOnceThenNeverRepoll {
            inner: Box::pin(fut),
            poll_senders,
            poll_pending_counts: 0,
        }
        .await
    });

    for (i, poll_receiver) in poll_receivers
        .iter_mut()
        .enumerate()
        .take(POLL_PENDING_COUNT)
    {
        // Wait until the inner open has been polled once and registered with io_uring.
        if i == 0 {
            poll_receiver.recv().await.unwrap();
        } else {
            // Wait until Tokio wakes the task because the open/statx completed. At this point
            // the CQE should already be stored as `Lifecycle::Completed(cqe)`.
            let _ = timeout(Duration::from_secs(2), poll_receiver.recv()).await;
        }
    }

    // Abort now, before the inner statx future gets re-polled and consumes the CQE.
    handle.abort();
    let err = handle.await.unwrap_err();
    assert!(err.is_cancelled(), "task was not cancelled as expected");
}

#[test]
fn uring_completed_then_dropped() {
    if !io_uring_supported() {
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let before = fd_count();
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        let file_number = 128;

        for _ in 0..file_number {
            completed_then_dropped_before_repoll(path.clone()).await;
        }

        assert_fds_are_not_leaking(before, file_number, 1).await
    });
}
