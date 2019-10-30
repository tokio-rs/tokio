#![cfg(unix)]
#![warn(rust_2018_idioms)]

use tokio::signal::unix::{signal, SignalKind};

#[test]
#[should_panic]
fn no_runtime_panics_creating_signals() {
    let _ = signal(SignalKind::hangup());
}
