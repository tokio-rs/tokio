#![cfg(unix)]
#![cfg(feature = "signal")]

mod support;

use std::convert::TryFrom;
use std::net;
use support::*;
use tokio::net::TcpListener;
use tokio_test::assert_err;

#[test]
fn no_runtime_fails_to_bind_resource() {
    let listener = net::TcpListener::bind("127.0.0.1:0").expect("failed to bind listener");
    assert_err!(TcpListener::try_from(listener));

    assert_err!(signal(SignalKind::hangup()));
}
