#![warn(rust_2018_idioms)]

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_test::assert_ok;

#[tokio::test]
async fn connect() {
    let addr = assert_ok!("127.0.0.1:0".parse());
    let mut srv = assert_ok!(TcpListener::bind(&addr));
    let addr = assert_ok!(srv.local_addr());

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (socket, addr) = assert_ok!(srv.accept().await);
        assert_eq!(addr, assert_ok!(socket.peer_addr()));
        assert_ok!(tx.send(socket));
    });

    let mine = assert_ok!(TcpStream::connect(&addr).await);
    let theirs = assert_ok!(rx.await);

    assert_eq!(
        assert_ok!(mine.local_addr()),
        assert_ok!(theirs.peer_addr())
    );
    assert_eq!(
        assert_ok!(theirs.local_addr()),
        assert_ok!(mine.peer_addr())
    );
}

/*
 * TODO: bring this back once TCP exposes HUP again
 *
#[cfg(target_os = "linux")]
mod linux {
    use tokio::net::{TcpListener, TcpStream};
    use tokio::prelude::*;
    use tokio_test::assert_ok;

    use mio::unix::UnixReady;

    use futures_util::future::poll_fn;
    use std::io::Write;
    use std::time::Duration;
    use std::{net, thread};

    #[tokio::test]
    fn poll_hup() {
        let addr = assert_ok!("127.0.0.1:0".parse());
        let mut srv = assert_ok!(TcpListener::bind(&addr));
        let addr = assert_ok!(srv.local_addr());

        tokio::spawn(async move {
            let (mut client, _) = assert_ok!(srv.accept().await);
            assert_ok!(client.set_linger(Some(Duration::from_millis(0))));
            assert_ok!(client.write_all(b"hello world").await);

            // TODO: Drop?
        });

        /*
        let t = thread::spawn(move || {
            let mut client = assert_ok!(srv.accept()).0;
            client.set_linger(Some(Duration::from_millis(0))).unwrap();
            client.write(b"hello world").unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        */

        let mut stream = assert_ok!(TcpStream::connect(&addr).await);

        // Poll for HUP before reading.
        future::poll_fn(|| stream.poll_read_ready(UnixReady::hup().into()))
            .wait()
            .unwrap();

        // Same for write half
        future::poll_fn(|| stream.poll_write_ready())
            .wait()
            .unwrap();

        let mut buf = vec![0; 11];

        // Read the data
        future::poll_fn(|| stream.poll_read(&mut buf))
            .wait()
            .unwrap();

        assert_eq!(b"hello world", &buf[..]);

        t.join().unwrap();
    }
}
*/
