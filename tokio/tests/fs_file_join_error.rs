#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

//! A `tokio::fs::File` op whose backing blocking task fails (cancelled/panicked
//! -> `JoinError`, or fails to spawn) must leave the File in a valid state and
//! return an `io::Error`, not poison it so a later op panics.
//!
//! We run each op on a runtime whose blocking pool is already shut down, that way
//! the task is cancelled on spawn. A poison panics here and fails the test.

use std::future::Future;
use std::io::SeekFrom::Start;
use std::path::Path;
use std::task::Context;
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::runtime::{Builder, Handle};

fn poll_once<F: Future>(fut: F) {
    let mut fut = Box::pin(fut);
    let _ = fut
        .as_mut()
        .poll(&mut Context::from_waker(futures::task::noop_waker_ref()));
}

/// A handle to a runtime whose blocking pool has already been shut down.
fn dead_pool_handle() -> Handle {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let h = rt.handle().clone();
    rt.shutdown_timeout(Duration::from_secs(0));
    h
}

/// Run `ops` in order on a fresh dead-pool File.
fn run(path: &Path, h: &Handle, ops: &[fn(&mut File)]) {
    let _enter = h.enter();
    let opts = std::fs::File::options()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut file = File::from_std(opts);
    for &op in ops {
        op(&mut file);
    }
}

#[test]
fn file_ops_survive_join_error() {
    let path = std::env::temp_dir().join(format!("tokio_join_error_{}", std::process::id()));
    std::fs::write(&path, vec![b'x'; 4096]).unwrap();
    let h = dead_pool_handle();

    let read: fn(&mut File) = |f| poll_once(f.read(&mut [0u8; 8]));
    let seek: fn(&mut File) = |f| poll_once(f.seek(Start(0)));
    let write: fn(&mut File) = |f| poll_once(f.write(b"z"));
    let flush: fn(&mut File) = |f| poll_once(f.flush());
    let set_len: fn(&mut File) = |f| poll_once(f.set_len(0));

    // Each op fails, however none may panic.
    run(&path, &h, &[read, read]);
    run(&path, &h, &[seek, seek]);
    run(&path, &h, &[set_len, set_len]);
    run(&path, &h, &[write, write]);
    run(&path, &h, &[read, flush]);
}
