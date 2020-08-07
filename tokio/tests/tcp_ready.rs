#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::pin::Pin;
use std::task::Poll;

use futures::future::poll_fn;
use mio::Ready;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::try_join;

use tokio_test::assert_ok;

macro_rules! assert_readable {
    ($stream:expr) => {
        let ready = assert_ok!(poll_fn(|cx| $stream.poll_read_ready(cx, Ready::readable())).await);
        assert!(ready.is_readable());
    };
}

macro_rules! assert_not_readable {
    ($stream:expr) => {
        poll_fn(|cx| {
            assert!($stream.poll_read_ready(cx, Ready::readable()).is_pending());
            Poll::Ready(())
        })
        .await;
    };
}

macro_rules! assert_writable {
    ($stream:expr) => {
        let ready = assert_ok!(poll_fn(|cx| $stream.poll_write_ready(cx)).await);
        assert!(ready.is_writable());
    };
}

macro_rules! assert_not_writable {
    ($stream:expr) => {
        poll_fn(|cx| {
            assert!($stream.poll_write_ready(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    };
}

async fn create_pair() -> (TcpStream, TcpStream) {
    let mut listener = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(listener.local_addr());
    let (client, (server, _)) =
        assert_ok!(try_join!(TcpStream::connect(&addr), listener.accept(),));
    (client, server)
}

async fn read_until_pending(mut stream: &mut TcpStream) {
    poll_fn(|cx| {
        let mut buf = vec![0; 1024 * 1024];
        loop {
            match Pin::new(&mut stream).poll_read(cx, &mut buf) {
                Poll::Pending => break,
                Poll::Ready(res) => assert!(res.is_ok()),
            }
        }
        Poll::Ready(())
    })
    .await;
}

async fn write_until_pending(mut stream: &mut TcpStream) {
    poll_fn(|cx| {
        let buf = vec![0; 1024 * 1024];
        loop {
            match Pin::new(&mut stream).poll_write(cx, &buf) {
                Poll::Pending => break,
                Poll::Ready(res) => assert!(res.is_ok()),
            }
        }
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn tcp_stream_poll_read_ready() {
    let (mut client, mut server) = create_pair().await;

    // Initial state - not readable.
    assert_not_readable!(server);

    // There is data in the buffer - readable.
    assert_ok!(client.write_all(b"ping").await);
    assert_readable!(server);

    // Readable until calls to `poll_read` return `Poll::Pending`.
    let mut buf = [0; 4];
    assert_ok!(server.read_exact(&mut buf).await);
    assert_readable!(server);
    read_until_pending(&mut server).await;
    assert_not_readable!(server);

    // Detect the client disconnect.
    drop(client);
    assert_readable!(server);
}

#[tokio::test]
async fn tcp_stream_poll_write_ready() {
    let (mut client, mut server) = create_pair().await;

    // Initial state - writable.
    assert_writable!(client);

    // No space to write - not writable.
    write_until_pending(&mut client).await;
    assert_not_writable!(client);

    assert_ok!(client.flush().await); // just to be sure

    // Not writable until calls to `poll_write` return `Poll::Pending`.
    read_until_pending(&mut server).await;
    assert_not_writable!(client);
    assert_ok!(client.write_all(b"ping").await);
    assert_writable!(client);

    write_until_pending(&mut client).await;
    assert_not_writable!(client);

    // Detect the server disconnect.
    drop(server);
    assert_writable!(client);
}
