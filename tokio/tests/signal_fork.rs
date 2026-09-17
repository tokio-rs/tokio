#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]
#![cfg(not(miri))] // No `sigaction` or `fork` on Miri.

//! Verifies that a process that has used Tokio can `fork(2)` and create a
//! fresh, fully working runtime in the child process.
//!
//! All scenarios run sequentially inside a single `#[test]`: forking while
//! sibling test threads run risks inheriting locks held by those threads,
//! which would deadlock the child. Keep this file to exactly one test.

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use tokio::signal::unix::{signal, SignalKind};

fn new_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

/// Registers a listener for `kind`, then raises the signal and awaits it
/// `rounds` times. Reports whether the listener observed every round.
///
/// Multiple rounds matter when the parent process also has a runtime
/// running: with a self-pipe incorrectly shared across a fork, each round is
/// an independent race between the two processes' drivers for the wakeup
/// byte, so N rounds catch a regression with probability ~(1 - p^N).
fn receives_signal_rounds(rt: &Runtime, kind: SignalKind, rounds: u32) -> bool {
    rt.block_on(async {
        let mut sig = signal(kind).expect("failed to register signal listener");
        for round in 0..rounds {
            send_signal(kind.as_raw_value());
            if tokio::time::timeout(Duration::from_secs(15), sig.recv())
                .await
                .is_err()
            {
                eprintln!(
                    "signal_fork: signal {} lost in round {round}",
                    kind.as_raw_value()
                );
                return false;
            }
        }
        true
    })
}

/// Single-round variant of [`receives_signal_rounds`].
fn receives_signal(rt: &Runtime, kind: SignalKind) -> bool {
    receives_signal_rounds(rt, kind, 1)
}

/// Creates a fresh runtime and reports whether it receives `kind`.
fn fresh_runtime_receives(kind: SignalKind) -> bool {
    let rt = new_runtime();
    receives_signal(&rt, kind)
}

/// Spawns a short-lived child process and reports whether waiting on it
/// succeeds. Exercises the process reaping machinery. Note that on Linux
/// this takes the pidfd path; the SIGCHLD reaper path is only covered on
/// platforms without pidfd support, such as macOS.
fn spawns_child_process(rt: &Runtime) -> bool {
    rt.block_on(async {
        let status = tokio::process::Command::new("true").status();
        match tokio::time::timeout(Duration::from_secs(15), status).await {
            Ok(Ok(status)) => status.success(),
            other => {
                eprintln!("signal_fork: waiting on a spawned process failed: {other:?}");
                false
            }
        }
    })
}

/// Runs `f` in a forked child process; returns true iff the child exited
/// with code 0. The child exits via `_exit`, skipping destructors and atexit
/// handlers, so it never touches state inherited from the parent on its way
/// out.
fn fork_and_check(f: impl FnOnce() -> bool) -> bool {
    unsafe {
        match libc::fork() {
            -1 => panic!("fork failed: {}", std::io::Error::last_os_error()),
            0 => {
                let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(false);
                libc::_exit(i32::from(!ok));
            }
            child => {
                // Reap with a timeout so a hung child fails the test instead
                // of hanging it forever.
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    let mut status = 0;
                    match libc::waitpid(child, &mut status, libc::WNOHANG) {
                        0 => {
                            if Instant::now() > deadline {
                                libc::kill(child, libc::SIGKILL);
                                libc::waitpid(child, &mut status, 0);
                                return false;
                            }
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        pid if pid == child => {
                            return libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
                        }
                        _ => panic!("waitpid failed: {}", std::io::Error::last_os_error()),
                    }
                }
            }
        }
    }
}

#[test]
fn fork_scenarios() {
    // Scenario 1: the parent uses signals, shuts its runtime down, and forks.
    // Fresh runtimes in the child, of both flavors, must receive signals.
    let rt = new_runtime();
    assert!(
        receives_signal(&rt, SignalKind::user_defined1()),
        "parent must receive signals before the fork",
    );
    drop(rt);

    assert!(
        fork_and_check(|| {
            if !fresh_runtime_receives(SignalKind::user_defined1()) {
                return false;
            }

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            receives_signal(&rt, SignalKind::user_defined1())
        }),
        "child created after parent runtime shutdown",
    );

    // Scenario 2: the parent runtime stays alive across the fork. The child
    // must not touch it, but a fresh runtime in the child must receive
    // signals and reap child processes, and the parent's signal handling
    // must keep working after the fork. Multiple rounds maximize the chance
    // of catching a wrongly shared pipe, see `receives_signal_rounds`.
    let rt = new_runtime();
    assert!(receives_signal(&rt, SignalKind::user_defined2()));
    // Give the parent's worker a moment to park. This only reduces the
    // chance of forking while it holds a lock; that risk is inherent to
    // forking a multi-threaded process.
    std::thread::sleep(Duration::from_millis(100));

    assert!(
        fork_and_check(|| {
            let child_rt = new_runtime();
            receives_signal_rounds(&child_rt, SignalKind::user_defined2(), 20)
                && spawns_child_process(&child_rt)
        }),
        "child created while the parent runtime is alive",
    );

    assert!(
        receives_signal(&rt, SignalKind::user_defined2()),
        "parent signal handling must survive the child's pipe replacement",
    );
    // Shut the parent runtime down before the remaining scenarios: scenario 3
    // requires that nothing in the parent consumes signal events between its
    // `raise` and the fork.
    drop(rt);

    // Scenario 3: a signal recorded in the parent but not yet broadcast at
    // the moment of the fork must not be delivered as a ghost signal to a
    // listener in the child, while a real signal raised in the child
    // afterwards must still be delivered.
    let rt = new_runtime();
    assert!(receives_signal(&rt, SignalKind::hangup()));
    // Drop the runtime so nothing consumes signal events anymore, then raise:
    // the process-global signal handler stays installed and records the event
    // as pending, where it remains until the fork.
    drop(rt);
    send_signal(libc::SIGHUP);

    assert!(
        fork_and_check(|| {
            let child_rt = new_runtime();
            child_rt.block_on(async {
                let mut sig =
                    signal(SignalKind::hangup()).expect("failed to register signal listener");
                // No SIGHUP was raised in the child, so nothing must arrive.
                if tokio::time::timeout(Duration::from_millis(500), sig.recv())
                    .await
                    .is_ok()
                {
                    eprintln!("signal_fork: ghost SIGHUP delivered to the child");
                    return false;
                }
                // A real SIGHUP raised in the child must still arrive,
                // proving the listener is alive and the silence above was
                // not a broken listener.
                send_signal(libc::SIGHUP);
                tokio::time::timeout(Duration::from_secs(15), sig.recv())
                    .await
                    .is_ok()
            })
        }),
        "child must not observe signals recorded by the parent before the fork",
    );

    // Scenario 4: two threads in the child create runtimes concurrently,
    // racing to re-initialize the process-global signal state.
    assert!(
        fork_and_check(|| {
            let threads: Vec<_> = (0..2)
                .map(|_| std::thread::spawn(|| fresh_runtime_receives(SignalKind::user_defined1())))
                .collect();
            threads.into_iter().all(|t| t.join().unwrap_or_default())
        }),
        "concurrent runtime creation in the child",
    );

    // Scenario 5: forking twice in a row; each generation gets its own pipe.
    assert!(
        fork_and_check(|| {
            fresh_runtime_receives(SignalKind::user_defined1())
                && fork_and_check(|| fresh_runtime_receives(SignalKind::user_defined1()))
        }),
        "grandchild runtimes must work as well",
    );
}
