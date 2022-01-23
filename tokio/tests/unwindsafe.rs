#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::panic::{RefUnwindSafe, UnwindSafe};
use tokio::task::spawn_blocking;

#[tokio::test]
async fn futures_are_unwind_safe() {
    unwind_safe_future(|| async {
        let _ = spawn_blocking(|| {}).await;
    })
    .await
}

async fn unwind_safe_future<F, Fut>(_: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()> + std::panic::UnwindSafe,
{
}

#[test]
fn net_types_are_unwind_safe() {
    is_unwind_safe::<tokio::net::TcpListener>();
    is_unwind_safe::<tokio::net::TcpSocket>();
    is_unwind_safe::<tokio::net::TcpStream>();
    is_unwind_safe::<tokio::net::UdpSocket>();
}

#[test]
#[cfg(unix)]
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
