//! Demonstrates pre-warming the Linux file descriptor table to avoid latency
//! spikes caused by file descriptor table growth in multi-threaded processes.
//!
//! On Linux, the kernel's FD table is grown lazily and protected by RCU
//! synchronization. In multi-threaded processes, when a syscall like `socket()`
//! triggers a table resize, the calling thread blocks until all RCU readers
//! quiesce. This can cause stalls of tens of milliseconds on tokio worker threads,
//! blocking the entire event loop (not just one task).
//!
//! The workaround is to force the kernel to expand the FD table once per process
//! (before any runtime starts), by duplicating an FD to a high slot and then
//! closing it. The kernel never shrinks the FD table during a process's lifetime,
//! so the capacity persists.
//!
//! This is most relevant for services that open many connections concurrently
//! (e.g. HTTP servers, connection pools). The pre-warm target should be at least
//! your expected peak FD count, and must not exceed `RLIMIT_NOFILE`.
//!
//! See: <https://github.com/tokio-rs/tokio/issues/7970>
//!
//! Usage:
//!
//!     cargo run --example prewarm-fd-table

#![warn(rust_2018_idioms)]

/// Pre-warms the FD table using `fcntl(F_DUPFD_CLOEXEC)` to duplicate an FD
/// into a high slot, expanding the table in a single syscall. `F_DUPFD_CLOEXEC`
/// allocates the lowest available FD >= `target`, so it never clobbers an
/// existing FD.
#[cfg(target_os = "linux")]
fn prewarm_fd_table(target: i32) -> std::io::Result<()> {
    use std::os::unix::io::{FromRawFd, OwnedFd};

    let dev_null = std::fs::File::open("/dev/null")?;
    let raw = unsafe {
        libc::fcntl(
            std::os::unix::io::AsRawFd::as_raw_fd(&dev_null),
            libc::F_DUPFD_CLOEXEC,
            target,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Close both FDs. The table capacity persists.
    let _owned = unsafe { OwnedFd::from_raw_fd(raw) };
    drop(dev_null);

    Ok(())
}

/// Fully safe alternative using only stdlib. Requires O(n) syscalls instead of
/// one, but avoids `unsafe` entirely.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn prewarm_fd_table_safe(target: i32) -> std::io::Result<()> {
    let f = std::fs::File::open("/dev/null")?;
    let _fds: Vec<_> = (0..target)
        .map(|_| f.try_clone())
        .collect::<Result<_, _>>()?;
    Ok(())
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        const FD_TARGET: i32 = 10_000;

        println!("Pre-warming FD table to {FD_TARGET} entries...");
        if let Err(e) = prewarm_fd_table(FD_TARGET) {
            eprintln!("Warning: failed to pre-warm FD table: {e}");
        } else {
            println!("FD table pre-warmed successfully.");
        }
    }

    // Build the runtime *after* pre-warming.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {});
}
