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
use tokio::process::Command;

#[test]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn process_spawned_outside_runtime() {
    let _ = Command::new("true")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn();
}
