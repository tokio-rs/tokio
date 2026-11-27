//! Uring fsync tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::future::pending;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinSet;
use tokio_test::assert_pending;

use crate::support::io_uring::uring_op_supported;

mod support {
    pub(crate) mod io_uring;
}

fn multi_rt(n: usize) -> Box<dyn Fn() -> Runtime> {
    Box::new(move || {
        Builder::new_multi_thread()
            .worker_threads(n)
            .enable_all()
            .build()
            .unwrap()
    })
}

fn current_rt() -> Box<dyn Fn() -> Runtime> {
    Box::new(|| Builder::new_current_thread().enable_all().build().unwrap())
}

fn rt_combinations() -> Vec<Box<dyn Fn() -> Runtime>> {
    vec![
        current_rt(),
        multi_rt(1),
        multi_rt(2),
        multi_rt(8),
        multi_rt(64),
        multi_rt(256),
    ]
}

fn fsync_in_use() -> bool {
    uring_op_supported(io_uring::opcode::Fsync::CODE)
}

#[test]
fn sync_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 64;

        let tmp: Vec<NamedTempFile> = (0..NUM_FILES)
            .map(|_| NamedTempFile::new().unwrap())
            .collect();

        rt.block_on(async {
            let mut tasks = JoinSet::new();

            for (i, tmp) in tmp.iter().enumerate() {
                let path = tmp.path().to_owned();
                tasks.spawn(async move {
                    let mut file = File::options().write(true).open(&path).await.unwrap();
                    file.write_all(b"hello, world!").await.unwrap();
                    if i % 2 == 0 {
                        file.sync_all().await.unwrap();
                    } else {
                        file.sync_data().await.unwrap();
                    }
                });
            }
            tasks.join_all().await;
        });

        for tmp in &tmp {
            assert_eq!(std::fs::read(tmp.path()).unwrap(), b"hello, world!");
        }
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn sync_waits_for_inflight_write() {
    let tmp = NamedTempFile::new().unwrap();
    let mut file = File::options()
        .read(true)
        .write(true)
        .open(tmp.path())
        .await
        .unwrap();

    // `write` only hands the data to the background task. `sync_all` must
    // wait for that write before flushing.
    file.write_all(b"hello, world!").await.unwrap();
    file.sync_all().await.unwrap();

    file.rewind().await.unwrap();
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await.unwrap();
    assert_eq!(contents, b"hello, world!");
}

#[tokio::test]
async fn sync_pipe_fails() {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: `pipe` returned two new descriptors that nothing else owns.
    let (read, _write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    let file = File::from_std(std::fs::File::from(read));

    let err = file.sync_all().await.unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EINVAL));

    let err = file.sync_data().await.unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EINVAL));
}

#[tokio::test]
async fn cancel_sync_keeps_file_usable() {
    if !fsync_in_use() {
        return;
    }

    let tmp = NamedTempFile::new().unwrap();
    let mut file = File::options().write(true).open(tmp.path()).await.unwrap();

    for _ in 0..100 {
        let mut fut = Box::pin(file.sync_all());
        assert_pending!(futures::poll!(fut.as_mut()));
        // The driver keeps the descriptor open until the fsync completes.
        drop(fut);
    }

    file.write_all(b"still open").await.unwrap();
    file.sync_all().await.unwrap();
    assert_eq!(std::fs::read(tmp.path()).unwrap(), b"still open");
}

#[test]
fn shutdown_runtime_while_performing_io_uring_ops() {
    if !fsync_in_use() {
        return;
    }

    fn run(rt: Runtime) {
        let (done_tx, done_rx) = mpsc::channel();
        let tmp = NamedTempFile::new().unwrap();
        const N: usize = 100;

        rt.block_on(async {
            let mut futs = vec![];

            for _ in 0..N {
                // One `File` per future, as `File` runs one operation at a time.
                let file = File::from_std(tmp.reopen().unwrap());
                let mut fut = Box::pin(async move { file.sync_all().await });

                assert_pending!(futures::poll!(fut.as_mut()));

                futs.push(fut);
            }

            tokio::spawn(async move {
                let _futs = futs;
                pending::<()>().await;
            });
        });

        std::thread::spawn(move || {
            rt.shutdown_timeout(Duration::from_millis(300));
            done_tx.send(()).unwrap();
        });

        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    for rt in rt_combinations() {
        run(rt());
    }
}
