//! Tests for `tokio::io::uring::Ring`.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::sync::Arc;
use tokio::io::uring::Ring;
use tokio_test::{assert_ready, task};

/// The offset that reads or writes at the file's current position.
const CURRENT: u64 = u64::MAX;

type Completion = (io::Result<usize>, Option<Vec<u8>>);

/// A new ring, or `None` where the current runtime can't use io_uring.
async fn ring() -> Option<Ring> {
    let mut ring = Ring::new();
    let [(res, _)] = completions(ring.nop().submit().await);
    match res {
        Ok(_) => Some(ring),
        Err(e) if e.kind() == io::ErrorKind::Unsupported => None,
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// The completions of a submission of `N` operations.
fn completions<const N: usize>(done: Vec<Completion>) -> [Completion; N] {
    done.try_into().expect("one completion per operation")
}

fn file_with(data: &[u8]) -> Arc<File> {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(data).unwrap();
    Arc::new(file)
}

/// Writes to it fail with `EBADF`.
fn read_only_file() -> Arc<File> {
    Arc::new(File::open("/dev/null").unwrap())
}

/// Reads from it fail with `EBADF`.
fn write_only_file() -> Arc<File> {
    Arc::new(OpenOptions::new().write(true).open("/dev/null").unwrap())
}

fn os_error<T: std::fmt::Debug>(res: io::Result<T>) -> i32 {
    res.unwrap_err().raw_os_error().unwrap()
}

fn kept_buffer() -> Vec<u8> {
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(b"kept");
    buf
}

#[test]
fn submit_fails_without_io_uring() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let [(read, buf)] = completions(
        rt.block_on(
            Ring::new()
                .read_at(file_with(b"abc"), kept_buffer(), 0)
                .submit(),
        ),
    );

    assert_eq!(read.unwrap_err().kind(), io::ErrorKind::Unsupported);
    assert_eq!(buf.unwrap(), b"kept");
}

#[test]
fn submit_fails_outside_a_runtime() {
    let mut submit = task::spawn(
        Ring::new()
            .read_at(file_with(b"abc"), kept_buffer(), 0)
            .submit(),
    );

    let [(read, buf)] = completions(assert_ready!(submit.poll()));
    read.unwrap_err();
    assert_eq!(buf.unwrap(), b"kept");
}

#[test]
fn empty_submission_outside_a_runtime() {
    let mut submit = task::spawn(Ring::new().submit());

    assert!(assert_ready!(submit.poll()).is_empty());
}

#[test]
fn submission_uses_the_runtime_that_polls_it() {
    // Built before any runtime exists.
    let submit = Ring::new()
        .read_at(file_with(b"abc"), Vec::with_capacity(16), 0)
        .submit();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let [(read, buf)] = completions(rt.block_on(submit));

    match read {
        // The host can't use io_uring.
        Err(e) if e.kind() == io::ErrorKind::Unsupported => return,
        read => assert_eq!(read.unwrap(), 3),
    }
    assert_eq!(buf.unwrap(), b"abc");
}

#[tokio::test]
async fn reads_complete_in_order() {
    let Some(mut ring) = ring().await else { return };
    let data: Vec<u8> = (0..4u8).flat_map(|i| [i; 1024]).collect();
    let file = file_with(&data);

    for i in 0..4u64 {
        ring.read_at(file.clone(), Vec::with_capacity(1024), i * 1024);
    }
    let done = ring.submit().await;

    assert_eq!(done.len(), 4);
    for (i, (read, buf)) in done.into_iter().enumerate() {
        assert_eq!(read.unwrap(), 1024);
        assert_eq!(buf.unwrap(), vec![i as u8; 1024]);
    }
}

#[tokio::test]
async fn batch_adds_the_operations_of_each_item() {
    let Some(mut ring) = ring().await else { return };
    let data: Vec<u8> = (0..4u8).flat_map(|i| [i; 1024]).collect();
    let file = file_with(&data);

    let done = ring
        .batch((0..4096).step_by(1024), |ring, offset| {
            ring.read_at(file.clone(), Vec::with_capacity(1024), offset)
        })
        .submit()
        .await;

    let mut contents = Vec::new();
    for (read, buf) in done {
        assert_eq!(read.unwrap(), 1024);
        contents.extend_from_slice(&buf.unwrap());
    }
    assert_eq!(contents, data);
}

#[tokio::test]
async fn batch_links_across_items() {
    let Some(mut ring) = ring().await else { return };
    let file = read_only_file();

    // Each item links its write to the next item's write, so the first
    // failure cancels the rest.
    let done = ring
        .batch(0..3, |ring, _| {
            ring.write_at(file.clone(), b"x".to_vec(), CURRENT).then()
        })
        .submit()
        .await;

    let [(first, _), (second, _), (third, _)] = completions(done);
    assert_eq!(os_error(first), libc::EBADF);
    assert_eq!(os_error(second), libc::ECANCELED);
    assert_eq!(os_error(third), libc::ECANCELED);
}

#[tokio::test]
async fn batch_of_no_items_adds_nothing() {
    let Some(mut ring) = ring().await else { return };

    let done = ring
        .batch(std::iter::empty::<u64>(), |ring, _| ring.nop())
        .submit()
        .await;

    assert!(done.is_empty());
}

#[tokio::test]
async fn read_appends_to_buffer() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(b"world");

    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(b"hello ");
    let [(read, buf)] = completions(ring.read_at(file, buf, 0).submit().await);

    assert_eq!(read.unwrap(), 5);
    assert_eq!(buf.unwrap(), b"hello world");
}

#[tokio::test]
async fn failed_read_gives_back_its_buffer() {
    let Some(mut ring) = ring().await else { return };

    let [(read, buf)] = completions(
        ring.read_at(write_only_file(), kept_buffer(), 0)
            .submit()
            .await,
    );

    assert_eq!(os_error(read), libc::EBADF);
    assert_eq!(buf.unwrap(), b"kept");
}

#[tokio::test]
async fn empty_submission() {
    let Some(mut ring) = ring().await else { return };

    assert!(ring.submit().await.is_empty());
}

#[tokio::test]
async fn submission_larger_than_submission_queue() {
    let Some(mut ring) = ring().await else { return };

    for _ in 0..1000 {
        ring.nop();
    }
    let done = ring.submit().await;

    assert_eq!(done.len(), 1000);
    assert!(done
        .into_iter()
        .all(|(res, buf)| res.is_ok() && buf.is_none()));
}

#[tokio::test]
async fn write_then_sync() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(b"");

    let [(written, buf), (synced, _)] = completions(
        ring.write_at(file.clone(), b"hello".to_vec(), 0)
            .then()
            .sync_data(file.clone())
            .submit()
            .await,
    );
    assert_eq!(written.unwrap(), 5);
    assert_eq!(buf.unwrap(), b"hello");
    synced.unwrap();

    // The ring is empty again after a submission, and can be reused.
    let [(read, buf)] = completions(ring.read_at(file, Vec::with_capacity(16), 0).submit().await);
    assert_eq!(read.unwrap(), 5);
    assert_eq!(buf.unwrap(), b"hello");
}

#[tokio::test]
async fn then_is_cancelled_after_a_failure() {
    let Some(mut ring) = ring().await else { return };

    let [(written, _), (next, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .then()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(os_error(written), libc::EBADF);
    assert_eq!(os_error(next), libc::ECANCELED);
}

#[tokio::test]
async fn always_runs_after_a_failure() {
    let Some(mut ring) = ring().await else { return };

    let [(written, _), (next, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .always()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(os_error(written), libc::EBADF);
    next.unwrap();
}

#[tokio::test]
async fn always_only_covers_the_previous_operation() {
    let Some(mut ring) = ring().await else { return };

    let [(written, _), (second, _), (third, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .then()
            .nop()
            .always()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(os_error(written), libc::EBADF);
    assert_eq!(os_error(second), libc::ECANCELED);
    assert_eq!(os_error(third), libc::ECANCELED);
}

#[tokio::test]
async fn always_then_then_runs_after_a_failure() {
    let Some(mut ring) = ring().await else { return };

    let [(written, _), (second, _), (third, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .always()
            .nop()
            .then()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(os_error(written), libc::EBADF);
    second.unwrap();
    third.unwrap();
}

#[tokio::test]
async fn short_read_breaks_a_link() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(b"abc");

    let [(read, buf), (next, _)] = completions(
        ring.read_at(file, Vec::with_capacity(16), 0)
            .then()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(read.unwrap(), 3);
    assert_eq!(buf.unwrap(), b"abc");
    assert_eq!(os_error(next), libc::ECANCELED);
}

#[tokio::test]
async fn link_from_the_last_operation_is_ignored() {
    let Some(mut ring) = ring().await else { return };

    let [(res, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .then()
            .submit()
            .await,
    );
    assert_eq!(os_error(res), libc::EBADF);

    // The next submission isn't linked to the failed write.
    let [(res, _)] = completions(ring.nop().submit().await);
    res.unwrap();
}

#[tokio::test]
async fn dropping_a_submission_keeps_its_operations() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(&[7; 4096]);

    for _ in 0..8 {
        ring.read_at(file.clone(), Vec::with_capacity(4096), 0);
    }
    let mut submit = task::spawn(ring.submit());
    assert!(submit.poll().is_pending());
    drop(submit);

    // The driver keeps the dropped reads until they complete, and the ring
    // keeps working.
    let [(res, _)] = completions(ring.nop().submit().await);
    res.unwrap();
}

#[tokio::test]
async fn dropping_a_submitted_chain_keeps_each_kind_of_operation() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(&[7; 4096]);

    ring.read_at(file.clone(), Vec::with_capacity(4096), 0)
        .then()
        .write_at(file.clone(), vec![1; 4096], 0)
        .then()
        .sync_all(file)
        .then()
        .nop();
    let mut submit = task::spawn(ring.submit());
    assert!(submit.poll().is_pending());
    drop(submit);

    // The driver keeps the dropped operations until they complete, and the
    // ring keeps working.
    let [(res, _)] = completions(ring.nop().submit().await);
    res.unwrap();
}

#[tokio::test]
async fn read_to_end_reads_the_whole_file() {
    let Some(mut ring) = ring().await else { return };
    let data: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();

    let [(read, buf)] = completions(
        ring.read_to_end(file_with(&data), Vec::new())
            .submit()
            .await,
    );

    assert_eq!(read.unwrap(), data.len());
    assert_eq!(buf.unwrap(), data);
}

#[tokio::test]
async fn read_to_end_appends_to_buffer() {
    let Some(mut ring) = ring().await else { return };

    let [(read, buf)] = completions(
        ring.read_to_end(file_with(b"world"), b"hello ".to_vec())
            .submit()
            .await,
    );

    assert_eq!(read.unwrap(), 5);
    assert_eq!(buf.unwrap(), b"hello world");
}

#[tokio::test]
async fn read_to_end_into_an_exact_fit() {
    let Some(mut ring) = ring().await else { return };
    let data = vec![7; 4096];

    // The buffer fills up exactly, so the read probes for the end of the file.
    let [(read, buf)] = completions(
        ring.read_to_end(file_with(&data), Vec::with_capacity(data.len()))
            .submit()
            .await,
    );

    assert_eq!(read.unwrap(), data.len());
    assert_eq!(buf.unwrap(), data);
}

#[tokio::test]
async fn read_to_end_doesnt_move_the_file_position() {
    let Some(mut ring) = ring().await else { return };
    // `file_with` leaves the position at the end of the file.
    let file = file_with(b"abc");

    let done = ring.read_to_end(file.clone(), Vec::new()).submit().await;
    let [(read, buf)] = completions(done);
    assert_eq!(read.unwrap(), 3);
    assert_eq!(buf.unwrap(), b"abc");

    // A read at the current position still finds the end of the file.
    let done = ring
        .read_at(file, Vec::with_capacity(16), CURRENT)
        .submit()
        .await;
    let [(read, buf)] = completions(done);
    assert_eq!(read.unwrap(), 0);
    assert!(buf.unwrap().is_empty());
}

#[tokio::test]
async fn read_to_end_alongside_other_operations() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(b"abcdef");

    let [(nop, _), (read_to_end, all), (read, part)] = completions(
        ring.nop()
            .read_to_end(file.clone(), Vec::new())
            .read_at(file, Vec::with_capacity(3), 3)
            .submit()
            .await,
    );

    nop.unwrap();
    assert_eq!(read_to_end.unwrap(), 6);
    assert_eq!(all.unwrap(), b"abcdef");
    assert_eq!(read.unwrap(), 3);
    assert_eq!(part.unwrap(), b"def");
}

#[tokio::test]
async fn only_reads_to_end() {
    let Some(mut ring) = ring().await else { return };

    let [(first, a), (second, b)] = completions(
        ring.read_to_end(file_with(b"a"), Vec::new())
            .read_to_end(file_with(b"bc"), Vec::new())
            .submit()
            .await,
    );

    assert_eq!(first.unwrap(), 1);
    assert_eq!(a.unwrap(), b"a");
    assert_eq!(second.unwrap(), 2);
    assert_eq!(b.unwrap(), b"bc");
}

#[tokio::test]
async fn links_to_and_from_read_to_end_are_ignored() {
    let Some(mut ring) = ring().await else { return };

    let [(written, _), (read, _), (next, _)] = completions(
        ring.write_at(read_only_file(), b"x".to_vec(), CURRENT)
            .then()
            .read_to_end(file_with(b"abc"), Vec::new())
            .then()
            .nop()
            .submit()
            .await,
    );

    assert_eq!(os_error(written), libc::EBADF);
    // Neither the read to end nor the nop is cancelled by the failed write.
    assert_eq!(read.unwrap(), 3);
    next.unwrap();
}

#[tokio::test]
async fn failed_read_to_end_drops_its_buffer() {
    let Some(mut ring) = ring().await else { return };

    let [(read, buf)] = completions(
        ring.read_to_end(write_only_file(), kept_buffer())
            .submit()
            .await,
    );

    assert_eq!(os_error(read), libc::EBADF);
    assert!(buf.is_none());
}

#[test]
fn read_to_end_gives_back_its_buffer_outside_a_runtime() {
    let mut submit = task::spawn(
        Ring::new()
            .read_to_end(file_with(b"abc"), kept_buffer())
            .submit(),
    );

    let [(read, buf)] = completions(assert_ready!(submit.poll()));
    read.unwrap_err();
    assert_eq!(buf.unwrap(), b"kept");
}

#[tokio::test]
async fn dropping_a_submission_with_a_read_to_end() {
    let Some(mut ring) = ring().await else { return };
    let file = file_with(&[7; 1 << 20]);

    let mut submit = task::spawn(ring.read_to_end(file, Vec::new()).nop().submit());
    assert!(submit.poll().is_pending());
    drop(submit);

    // The dropped read's operation is kept until it completes, and the ring
    // keeps working.
    let [(res, _)] = completions(ring.nop().submit().await);
    res.unwrap();
}
