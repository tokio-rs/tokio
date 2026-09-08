#![warn(rust_2018_idioms)]
#![cfg(any(
    all(feature = "full", not(target_os = "wasi")), // Wasi does not support panic recovery
    all(target_os = "emscripten", feature = "rt", feature = "sync")
))]

use std::panic::{RefUnwindSafe, UnwindSafe};

#[test]
fn notify_is_unwind_safe() {
    is_unwind_safe::<tokio::sync::Notify>();
}

#[test]
fn join_handle_is_unwind_safe() {
    is_unwind_safe::<tokio::task::JoinHandle<()>>();
}

#[test]
#[cfg(feature = "net")]
fn net_types_are_unwind_safe() {
    is_unwind_safe::<tokio::net::TcpListener>();
    is_unwind_safe::<tokio::net::TcpSocket>();
    is_unwind_safe::<tokio::net::TcpStream>();
    is_unwind_safe::<tokio::net::UdpSocket>();
}

#[test]
#[cfg(all(unix, feature = "net"))]
fn unix_net_types_are_unwind_safe() {
    is_unwind_safe::<tokio::net::UnixDatagram>();
    is_unwind_safe::<tokio::net::UnixListener>();
    is_unwind_safe::<tokio::net::UnixStream>();
}

#[test]
#[cfg(windows)]
fn windows_net_types_are_unwind_safe() {
    use tokio::net::windows::named_pipe::NamedPipeClient;
    use tokio::net::windows::named_pipe::NamedPipeServer;

    is_unwind_safe::<NamedPipeClient>();
    is_unwind_safe::<NamedPipeServer>();
}

fn is_unwind_safe<T: UnwindSafe + RefUnwindSafe>() {}
