#![warn(rust_2018_idioms)]
#![cfg(all(
    target_os = "linux",
    feature = "full",
    not(miri),
    not(tokio_no_tuning_tests),
    panic = "unwind",
))]

use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use tokio::runtime::Builder;
use tokio::time::{sleep, timeout};

#[test]
fn spawn_blocking_with_only_scheduler_workers() {
    // Regression test for https://github.com/tokio-rs/tokio/issues/8406.
    //
    // On a multi-threaded runtime, the only pool threads present before the
    // first `spawn_blocking` are the scheduler workers themselves. If the OS
    // then refuses to create a new thread, the pool used to swallow the error
    // assuming a busy thread would pick up the task, but scheduler workers
    // never drain the blocking queue, so the task was orphaned forever.
    //
    // Pin the per-user process thread limit to the current thread count so
    // that creating a new pool thread fails; `spawn_blocking` must panic
    // with "OS can't spawn worker thread" instead of hanging.
    let rt = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async { sleep(Duration::from_millis(300)).await });

    let threads = std::fs::read_dir("/proc/self/task").unwrap().count();
    let lim = libc::rlimit {
        rlim_cur: threads as libc::rlim_t,
        rlim_max: threads as libc::rlim_t,
    };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &lim) }, 0);

    // If the limit could not be made effective (e.g. the host user is
    // already near its thread cap), skip rather than flake.
    match std::thread::Builder::new()
        .name("probe".into())
        .spawn(|| {})
    {
        Ok(h) => {
            let _ = h.join();
            eprintln!("skip: setrlimit did not prevent thread creation");
            return;
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(e) => panic!("unexpected thread-spawn error: {e}"),
    }

    let res = panic::catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(async {
            timeout(
                Duration::from_secs(5),
                tokio::task::spawn_blocking(|| 42u32),
            )
            .await
        })
    }));

    // The fixed code panics synchronously when no real pool thread can be
    // created; the bug let the task hang, which the timeout turns into
    // `Ok(Err(_))`.
    match res {
        Err(_) => {}
        Ok(Err(_)) => panic!("spawn_blocking timed out"),
        Ok(Ok(_)) => panic!("spawn_blocking unexpectedly succeeded"),
    }
}
