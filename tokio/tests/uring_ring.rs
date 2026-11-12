//! Tests for `tokio::io::uring::Ring`.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::fs::File;
use std::io::{self, Write as _};
use std::sync::Arc;
use tokio::io::uring::{Nop, Ring};
use tokio_test::task;

/// The ring of the current runtime, or `None` where io_uring is unavailable.
fn ring() -> Option<Ring> {
    Ring::current().ok()
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

fn os_error<T: std::fmt::Debug>(res: io::Result<T>) -> i32 {
    res.unwrap_err().raw_os_error().unwrap()
}

#[test]
fn current_fails_without_io() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        let err = Ring::current().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    });
}

#[tokio::test]
async fn batch_reads_chunks_in_order() {
    let Some(ring) = ring() else { return };
    let data: Vec<u8> = (0..4u8).flat_map(|i| [i; 1024]).collect();
    let file = file_with(&data);

    let chunks = ring
        .batch((0..4u64).map(|i| ring.read(&file, Vec::with_capacity(1024)).at(i * 1024)))
        .await;

    assert_eq!(chunks.len(), 4);
    for (i, chunk) in chunks.into_iter().enumerate() {
        assert_eq!(chunk.unwrap(), vec![i as u8; 1024]);
    }
}

#[tokio::test]
async fn read_appends_to_buffer() {
    let Some(ring) = ring() else { return };
    let file = file_with(b"world");

    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(b"hello ");
    let (read,) = ring.chain(ring.read(&file, buf).at(0)).await;

    assert_eq!(read.unwrap(), b"hello world");
}

#[tokio::test]
async fn empty_batch() {
    let Some(ring) = ring() else { return };

    let results = ring.batch(std::iter::empty::<Nop>()).await;

    assert!(results.is_empty());
}

#[tokio::test]
async fn batch_larger_than_submission_queue() {
    let Some(ring) = ring() else { return };

    let results = ring.batch((0..1000).map(|_| ring.nop())).await;

    assert_eq!(results.len(), 1000);
    assert!(results.into_iter().all(|res| res.is_ok()));
}

#[tokio::test]
async fn chain_writes_then_syncs() {
    let Some(ring) = ring() else { return };
    let file = file_with(b"");

    let (written, synced) = ring
        .chain(ring.write(&file, b"hello".to_vec()).at(0))
        .then(ring.fsync(&file).data_only())
        .await;
    assert_eq!(written.unwrap(), 5);
    synced.unwrap();

    let (read,) = ring
        .chain(ring.read(&file, Vec::with_capacity(16)).at(0))
        .await;
    assert_eq!(read.unwrap(), b"hello");
}

#[tokio::test]
async fn then_is_cancelled_after_a_failure() {
    let Some(ring) = ring() else { return };
    let file = read_only_file();

    let (written, next) = ring
        .chain(ring.write(&file, b"x".to_vec()))
        .then(ring.nop())
        .await;

    assert_eq!(os_error(written), libc::EBADF);
    assert_eq!(os_error(next), libc::ECANCELED);
}

#[tokio::test]
async fn always_runs_after_a_failure() {
    let Some(ring) = ring() else { return };
    let file = read_only_file();

    let (written, next) = ring
        .chain(ring.write(&file, b"x".to_vec()))
        .always(ring.nop())
        .await;

    assert_eq!(os_error(written), libc::EBADF);
    next.unwrap();
}

#[tokio::test]
async fn always_only_covers_the_previous_operation() {
    let Some(ring) = ring() else { return };
    let file = read_only_file();

    let (written, second, third) = ring
        .chain(ring.write(&file, b"x".to_vec()))
        .then(ring.nop())
        .always(ring.nop())
        .await;

    assert_eq!(os_error(written), libc::EBADF);
    assert_eq!(os_error(second), libc::ECANCELED);
    assert_eq!(os_error(third), libc::ECANCELED);
}

#[tokio::test]
async fn short_read_breaks_a_link() {
    let Some(ring) = ring() else { return };
    let file = file_with(b"abc");

    let (read, next) = ring
        .chain(ring.read(&file, Vec::with_capacity(16)).at(0))
        .then(ring.nop())
        .await;

    assert_eq!(read.unwrap(), b"abc");
    assert_eq!(os_error(next), libc::ECANCELED);
}

#[tokio::test]
async fn dropping_a_submitted_batch_keeps_its_operations() {
    let Some(ring) = ring() else { return };
    let file = file_with(&[7; 4096]);

    let mut batch =
        task::spawn(ring.batch((0..8).map(|_| ring.read(&file, Vec::with_capacity(4096)).at(0))));
    assert!(batch.poll().is_pending());
    drop(batch);

    // The driver keeps the dropped reads until they complete, and the ring
    // keeps working.
    let (res,) = ring.chain(ring.nop()).await;
    res.unwrap();
}
