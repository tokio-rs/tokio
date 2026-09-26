#![warn(rust_2018_idioms)]
// WASIp1 doesn't support bind
#![cfg(all(
    feature = "net",
    feature = "macros",
    feature = "rt",
    feature = "io-util",
    not(all(target_os = "wasi", target_env = "p1")),
))]

use tokio::net::TcpListener;

use std::net;

#[test]
#[should_panic]
fn no_runtime_panics_binding_net_tcp_listener() {
    let listener = net::TcpListener::bind("127.0.0.1:0").expect("failed to bind listener");
    let _ = TcpListener::try_from(listener);
}
