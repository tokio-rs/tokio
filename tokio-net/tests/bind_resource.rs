#![cfg(unix)]
#![cfg(feature = "signal")]

mod support;

use std::convert::TryFrom;
use std::net;
use support::*;
use tokio::net::TcpListener;

#[test]
#[should_panic]
fn no_runtime_panics_binding_net_tcp_listener() {
    let listener = net::TcpListener::bind("127.0.0.1:0").expect("failed to bind listener");
    let _ = TcpListener::try_from(listener);
}

#[test]
#[should_panic]
fn no_runtime_panics_creating_signals() {
    let _ = signal(SignalKind::hangup());
}
