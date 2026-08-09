#![warn(rust_2018_idioms)]
// WASIp1 doesn't support bind
#![cfg(all(
    feature = "net",
    feature = "macros",
    feature = "rt",
    feature = "io-util",
    not(all(target_os = "wasi", target_env = "p1")),
))]

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_test::assert_ok;

#[tokio::test]
async fn echo_server() {
    const BYTES: &[u8] = b"foo bar baz";
    #[cfg(not(miri))]
    const ITER: usize = 1024;
    #[cfg(miri)] // Use a lower iteration count with Miri because it's too slow otherwise.
    const ITER: usize = 32;

    let (tx, rx) = oneshot::channel();

    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);

        for _ in 0..ITER {
            // write
            assert_ok!(stream.write_all(BYTES).await);

            // read
            let mut buf = [0; BYTES.len()];
            assert_ok!(stream.read_exact(&mut buf).await);
            assert_eq!(&buf[..], BYTES);
        }

        assert_ok!(tx.send(()));
    });

    let (mut stream, _) = assert_ok!(srv.accept().await);
    let (mut rd, mut wr) = stream.split();

    let n = assert_ok!(io::copy(&mut rd, &mut wr).await);
    assert_eq!(n, (ITER * BYTES.len()) as u64);

    assert_ok!(rx.await);
}
