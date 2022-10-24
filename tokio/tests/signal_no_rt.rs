#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]

use tokio::signal::unix::{signal, SignalKind};

#[cfg_attr(tokio_wasi, ignore = "Wasi does not support panic recovery")]
#[test]
#[should_panic]
fn no_runtime_panics_creating_signals() {
    let _ = signal(SignalKind::hangup());
}
