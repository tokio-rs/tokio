#![warn(rust_2018_idioms)]
#![cfg(all(
    target_os = "linux",
    feature = "full",
    not(miri),
    not(tokio_no_tuning_tests),
))]

// In a file of its own: it changes the process's thread limit, which the
// tests in `rt_blocking_thread_exhaust.rs` lower for good.

use std::sync::mpsc;
use std::time::Duration;

use tokio::runtime::Builder;
use tokio::time::timeout;

#[test]
fn spawn_blocking_waits_out_a_brief_thread_exhaustion() {
    // When the OS refuses a thread only for a moment (EAGAIN while the
    // system is short of processes or memory), `spawn_blocking` on a pool
    // with no blocking thread retries the start for a short while instead
    // of panicking.
    //
    // Pin the soft per-user thread limit to the current thread count, as
    // above, and lift it again 100 ms later from a thread started before
    // the limit was lowered; `spawn_blocking` must then succeed.

    let (started_tx, started_rx) = mpsc::sync_channel(4);
    let rt = Builder::new_multi_thread()
        .worker_threads(4)
        .on_thread_start(move || {
            started_tx.send(()).unwrap();
        })
        .enable_all()
        .build()
        .unwrap();

    for _ in 0..4 {
        started_rx.recv().unwrap();
    }

    let mut original = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NPROC, &mut original) },
        0
    );
    let (lowered_tx, lowered_rx) = mpsc::sync_channel::<()>(0);
    let lifter = std::thread::spawn(move || {
        lowered_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &original) }, 0);
    });

    let threads = std::fs::read_dir("/proc/self/task").unwrap().count();
    let lowered = libc::rlimit {
        rlim_cur: threads as libc::rlim_t,
        rlim_max: original.rlim_max,
    };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &lowered) }, 0);

    // If the limit is not enforced for this process (e.g. when running as root
    // or in a container), skip rather than fail.
    match std::thread::Builder::new()
        .name("probe".into())
        .spawn(|| {})
    {
        Ok(h) => {
            let _ = h.join();
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &original) }, 0);
            lowered_tx.send(()).unwrap();
            lifter.join().unwrap();
            eprintln!("skip: setrlimit did not prevent thread creation");
            return;
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(e) => panic!("unexpected thread-spawn error: {e}"),
    }

    lowered_tx.send(()).unwrap();
    let answer = rt.block_on(async {
        timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(|| 42u32),
        )
        .await
    });
    lifter.join().unwrap();
    assert_eq!(answer.expect("spawn_blocking timed out").unwrap(), 42);
}
