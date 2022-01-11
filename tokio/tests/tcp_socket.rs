#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::time::Duration;
use tokio::net::TcpSocket;
use tokio_test::assert_ok;

#[tokio::test]
async fn basic_usage_v4() {
    // Create server
    let addr = assert_ok!("127.0.0.1:0".parse());
    let srv = assert_ok!(TcpSocket::new_v4());
    assert_non_blocking(&srv);
    assert_close_on_exec(&srv);
    assert_ok!(srv.bind(addr));

    let srv = assert_ok!(srv.listen(128));

    // Create client & connect
    let addr = srv.local_addr().unwrap();
    let cli = assert_ok!(TcpSocket::new_v4());
    let _cli = assert_ok!(cli.connect(addr).await);

    // Accept
    let _ = assert_ok!(srv.accept().await);
}

#[tokio::test]
async fn basic_usage_v6() {
    // Create server
    let addr = assert_ok!("[::1]:0".parse());
    let srv = assert_ok!(TcpSocket::new_v6());
    assert_non_blocking(&srv);
    assert_close_on_exec(&srv);
    assert_ok!(srv.bind(addr));

    let srv = assert_ok!(srv.listen(128));

    // Create client & connect
    let addr = srv.local_addr().unwrap();
    let cli = assert_ok!(TcpSocket::new_v6());
    let _cli = assert_ok!(cli.connect(addr).await);

    // Accept
    let _ = assert_ok!(srv.accept().await);
}

#[tokio::test]
async fn bind_before_connect() {
    // Create server
    let any_addr = assert_ok!("127.0.0.1:0".parse());
    let srv = assert_ok!(TcpSocket::new_v4());
    assert_non_blocking(&srv);
    assert_close_on_exec(&srv);
    assert_ok!(srv.bind(any_addr));

    let srv = assert_ok!(srv.listen(128));

    // Create client & connect
    let addr = srv.local_addr().unwrap();
    let cli = assert_ok!(TcpSocket::new_v4());
    assert_ok!(cli.bind(any_addr));
    let _cli = assert_ok!(cli.connect(addr).await);

    // Accept
    let _ = assert_ok!(srv.accept().await);
}

#[tokio::test]
async fn basic_linger() {
    // Create server
    let addr = assert_ok!("127.0.0.1:0".parse());
    let srv = assert_ok!(TcpSocket::new_v4());
    assert_non_blocking(&srv);
    assert_close_on_exec(&srv);
    assert_ok!(srv.bind(addr));

    assert!(srv.linger().unwrap().is_none());

    srv.set_linger(Some(Duration::new(0, 0))).unwrap();
    assert_eq!(srv.linger().unwrap(), Some(Duration::new(0, 0)));
}

fn assert_non_blocking(socket: &TcpSocket) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
        assert!(flags & libc::O_NONBLOCK != 0, "O_NONBLOCK not set");
    }
    #[cfg(windows)]
    {
        // There is no way to assert this in windows.
        let _ = socket;
    }
}
fn assert_close_on_exec(socket: &TcpSocket) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFD) };
        assert!(flags & libc::FD_CLOEXEC != 0, "FD_CLOEXEC not set");
    }
    #[cfg(windows)]
    {
        // This flag is not present in windows.
        let _ = socket;
    }
}
