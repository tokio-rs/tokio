extern crate tokio;

#[test]
#[cfg(feature = "full")]
fn sync_io() {
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::net::TcpStream;
    use tokio::sync::lock::LockGuard;

    fn check<T: AsyncRead + AsyncWrite>() {}
    check::<LockGuard<TcpStream>>();
    check::<TcpStream>();
}
