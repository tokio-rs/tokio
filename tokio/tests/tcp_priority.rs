#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", any(target_os = "linux", target_os = "android")))]

use socket2::SockRef;
use std::mem::MaybeUninit;
use std::os::unix::io::AsRawFd;
use tokio::io::{self, Interest};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn await_priority() {
    use std::net::SocketAddr;

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client = TcpStream::connect(addr).await.unwrap();

    let (stream, _) = listener.accept().await.unwrap();

    // Sending out of band data should trigger priority event.
    send_oob_data(&stream, &[41, 42]).unwrap();

    let mut buf = [MaybeUninit::new(0u8); 5];

    // using try_io here (instead of e.g. `client.async_io`) for better test coverage
    let bytes_received = loop {
        client.ready(Interest::PRIORITY).await.unwrap();

        let result = client.try_io(Interest::PRIORITY, || {
            SockRef::from(&client).recv_out_of_band(&mut buf)
        });

        match result {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            other => break other.unwrap(),
        }
    };

    // only the most recent byte is received OOB
    assert_eq!(bytes_received, 1);
    assert_eq!(unsafe { buf[0].assume_init() }, 42);
}

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
