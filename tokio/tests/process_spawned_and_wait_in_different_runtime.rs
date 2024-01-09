#![cfg(feature = "process")]
#![warn(rust_2018_idioms)]
// This test reveals a difference in behavior of kqueue on FreeBSD. When the
// reader disconnects, there does not seem to be an `EVFILT_WRITE` filter that
// is returned.
//
// It is expected that `EVFILT_WRITE` would be returned with either the
// `EV_EOF` or `EV_ERROR` flag set. If either flag is set a write would be
// attempted, but that does not seem to occur.
#![cfg(all(unix, not(target_os = "freebsd")))]

use std::process::Stdio;
use tokio::{process::Command, runtime::Runtime};

#[test]
fn process_spawned_and_wait_in_different_runtime() {
    let mut child = Runtime::new().unwrap().block_on(async {
        Command::new("true")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    });
    Runtime::new().unwrap().block_on(async {
        let _ = child.wait().await.unwrap();
    });
}
