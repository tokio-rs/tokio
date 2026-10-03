//! Uring file operations tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use futures::future::FutureExt;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use std::{future::poll_fn, path::PathBuf};
use tempfile::NamedTempFile;
use tokio::{
    fs::OpenOptions,
    runtime::{Builder, Runtime},
};
use tokio_util::task::TaskTracker;

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

#[test]
fn shutdown_runtime_while_performing_io_uring_ops() {
    fn run(rt: Runtime) {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let (_tmp, path) = create_tmp_files(1);
        rt.spawn(async move {
            let path = path[0].clone();

            // spawning a bunch of uring operations.
            loop {
                let path = path.clone();
                tokio::spawn(async move {
                    let mut opt = OpenOptions::new();
                    opt.read(true);
                    opt.open(&path).await.unwrap();
                });

                // Avoid busy looping.
                tokio::task::yield_now().await;
            }
        });

        std::thread::spawn(move || {
            let rt: Runtime = rx.recv().unwrap();
            rt.shutdown_timeout(Duration::from_millis(300));
            done_tx.send(()).unwrap();
        });

        tx.send(rt).unwrap();
        done_rx.recv().unwrap();
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[test]
fn open_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 512;

        let (_tmp_files, paths): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(NUM_FILES);

        rt.block_on(async move {
            let tracker = TaskTracker::new();

            for i in 0..10_000 {
                let path = paths.get(i % NUM_FILES).unwrap().clone();
                tracker.spawn(async move {
                    let _file = OpenOptions::new().read(true).open(path).await.unwrap();
                });
            }
            tracker.close();
            tracker.wait().await;
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn cancel_op_future() {
    let (_tmp_file, path): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(1);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let opt = {
                let mut opt = tokio::fs::OpenOptions::new();
                opt.read(true);
                opt
            };

            let fut = opt.open(&path[0]);

            // If io_uring is enabled (and not falling back to the thread pool),
            // the first poll should return Pending.
            let _pending = Box::pin(fut).poll_unpin(cx);

            tx.send(()).unwrap();

            Poll::<()>::Pending
        })
        .await;
    });

    // Wait for the first poll
    rx.recv().await.unwrap();

    handle.abort();

    let res = handle.await.unwrap_err();
    assert!(res.is_cancelled());
}

#[cfg(panic = "unwind")]
#[test]
fn completion_waker_panic_preserves_uring() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;
    use std::task::{Context, Wake, Waker};

    struct PanicWaker;
    impl Wake for PanicWaker {
        fn wake(self: Arc<Self>) {
            panic!("completion waker panic");
        }
    }

    if io_uring::IoUring::new(2).is_err() {
        return;
    }

    let rt = Builder::new_current_thread().enable_all().build().unwrap();

    // Initialize io_uring on the runtime.
    rt.block_on(tokio::fs::File::open("/dev/null")).unwrap();

    let mut open_fut = Box::pin(tokio::fs::File::open("/dev/null"));
    {
        let _enter = rt.enter();
        let waker = Waker::from(Arc::new(PanicWaker));
        let mut cx = Context::from_waker(&waker);
        tokio_test::assert_pending!(open_fut.as_mut().poll_unpin(&mut cx));
    }

    let res = catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(std::future::pending::<()>());
    }));
    assert!(res.is_err());

    // The completed operation recorded its CQE before waking, and the ring is still intact.
    {
        let _enter = rt.enter();
        open_fut.now_or_never().unwrap().unwrap();

        let mut next_open = Box::pin(tokio::fs::File::open("/dev/null"));
        let mut cx = Context::from_waker(Waker::noop());
        tokio_test::assert_pending!(next_open.as_mut().poll_unpin(&mut cx));
    }
}

fn create_tmp_files(num_files: usize) -> (Vec<NamedTempFile>, Vec<PathBuf>) {
    let mut files = Vec::with_capacity(num_files);
    for _ in 0..num_files {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        files.push((tmp, path));
    }

    files.into_iter().unzip()
}
