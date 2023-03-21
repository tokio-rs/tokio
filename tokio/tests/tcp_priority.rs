#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(tokio_wasi)))] // Wasi doesn't support bind

use std::os::fd::AsRawFd;
use tokio::io::{self, Interest};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
#[cfg(any(target_os = "linux", target_os = "android"))]
async fn await_priority() {
    use std::net::SocketAddr;

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client = TcpStream::connect(addr).await.unwrap();

    let (stream, _) = listener.accept().await.unwrap();

    // Sending out of band data should trigger priority event.
    send_oob_data(&stream, b"hello").unwrap();

    let _ = client.ready(Interest::PRIORITY).await.unwrap();
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn send_oob_data<S: AsRawFd>(stream: &S, data: &[u8]) -> io::Result<usize> {
    unsafe {
        let res = libc::send(
            stream.as_raw_fd(),
            data.as_ptr().cast(),
            data.len(),
            libc::MSG_OOB,
        );
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res as usize)
        }
    }
}
