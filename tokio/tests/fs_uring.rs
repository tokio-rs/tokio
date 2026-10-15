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

#[cfg(all(feature = "signal", feature = "time"))]
#[tokio::test]
async fn shutdown_retries_interrupted_wait() {
    use std::os::unix::thread::JoinHandleExt;
    use tokio::signal::unix::{signal, SignalKind};
    use tokio::time::timeout;

    const TIMEOUT: Duration = Duration::from_secs(10);

    let Ok(ring) = io_uring::IoUring::new(2) else {
        return;
    };
    let mut probe = io_uring::Probe::new();
    if ring.submitter().register_probe(&mut probe).is_err()
        || !probe.is_supported(io_uring::opcode::OpenAt::CODE)
    {
        return;
    }
    drop(ring);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fifo");
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let mut signal = signal(SignalKind::user_defined1()).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let open_path = path.clone();
    let shutdown = std::thread::spawn(move || {
        let rt = current_rt()();
        // Initialise io_uring before polling the operation we will cancel.
        rt.block_on(tokio::fs::File::open("/dev/null")).unwrap();
        {
            let _enter = rt.enter();
            // O_CREAT forces io_uring to use its blocking open path for the FIFO.
            let mut open = tokio_test::task::spawn(tokio::fs::File::create(open_path));
            tokio_test::assert_pending!(open.poll());
        }
        // SAFETY: gettid take no arguments and returns this thread's ID.
        tx.send(unsafe { libc::syscall(libc::SYS_gettid) }).unwrap();
        drop(rt);
    });

    let pthread = shutdown.as_pthread_t();
    let interrupt_result = tokio::spawn(timeout(TIMEOUT, async move {
        let tid = rx.await.unwrap();
        let syscall_path = format!("/proc/self/task/{tid}/syscall");
        let syscall_number = format!("{} ", libc::SYS_io_uring_enter);
        let wait_for_uring_enter = || async {
            while !std::fs::read_to_string(&syscall_path)
                .unwrap()
                .starts_with(&syscall_number)
            {
                tokio::task::yield_now().await;
            }
        };
        // The FIFO has no reader, so shutdown must wait for the cancelled open.
        wait_for_uring_enter().await;
        // SAFETY: the join handle keeps the thread ID valid, and SIGUSR1 has a handler.
        assert_eq!(
            unsafe { libc::pthread_kill(pthread as libc::pthread_t, libc::SIGUSR1) },
            0
        );
        signal.recv().await.unwrap();
        // After signal delivry, another wait proves shutdown retried before we release the FIFO.
        wait_for_uring_enter().await;
    }))
    .await;

    // Release the FIFO before propagating failures. O_RDWR never waits for a peer.
    let _reader = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    timeout(TIMEOUT, async {
        while !shutdown.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime shutdown did not complete");
    shutdown
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    interrupt_result.unwrap().unwrap();
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
