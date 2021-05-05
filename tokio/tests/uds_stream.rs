#![cfg(feature = "full")]
#![warn(rust_2018_idioms)]
#![cfg(unix)]

use std::io;
use std::task::Poll;

use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::{UnixListener, UnixStream};
use tokio_test::{assert_ok, assert_pending, assert_ready_ok, task};

use futures::future::{poll_fn, try_join};

#[tokio::test]
async fn accept_read_write() -> std::io::Result<()> {
    let dir = tempfile::Builder::new()
        .prefix("tokio-uds-tests")
        .tempdir()
        .unwrap();
    let sock_path = dir.path().join("connect.sock");

    let listener = UnixListener::bind(&sock_path)?;

    let accept = listener.accept();
    let connect = UnixStream::connect(&sock_path);
    let ((mut server, _), mut client) = try_join(accept, connect).await?;

    // Write to the client. TODO: Switch to write_all.
    let write_len = client.write(b"hello").await?;
    assert_eq!(write_len, 5);
    drop(client);
    // Read from the server. TODO: Switch to read_to_end.
    let mut buf = [0u8; 5];
    server.read_exact(&mut buf).await?;
    assert_eq!(&buf, b"hello");
    let len = server.read(&mut buf).await?;
    assert_eq!(len, 0);
    Ok(())
}

#[tokio::test]
async fn shutdown() -> std::io::Result<()> {
    let dir = tempfile::Builder::new()
        .prefix("tokio-uds-tests")
        .tempdir()
        .unwrap();
    let sock_path = dir.path().join("connect.sock");

    let listener = UnixListener::bind(&sock_path)?;

    let accept = listener.accept();
    let connect = UnixStream::connect(&sock_path);
    let ((mut server, _), mut client) = try_join(accept, connect).await?;

    // Shut down the client
    AsyncWriteExt::shutdown(&mut client).await?;
    // Read from the server should return 0 to indicate the channel has been closed.
    let mut buf = [0u8; 1];
    let n = server.read(&mut buf).await?;
    assert_eq!(n, 0);
    Ok(())
}

#[tokio::test]
async fn try_read_write() -> std::io::Result<()> {
    let msg = b"hello world";

    let dir = tempfile::tempdir()?;
    let bind_path = dir.path().join("bind.sock");

    // Create listener
    let listener = UnixListener::bind(&bind_path)?;

    // Create socket pair
    let client = UnixStream::connect(&bind_path).await?;

    let (server, _) = listener.accept().await?;
    let mut written = msg.to_vec();

    // Track the server receiving data
    let mut readable = task::spawn(server.readable());
    assert_pending!(readable.poll());

    // Write data.
    client.writable().await?;
    assert_eq!(msg.len(), client.try_write(msg)?);

    // The task should be notified
    while !readable.is_woken() {
        tokio::task::yield_now().await;
    }

    // Fill the write buffer
    loop {
        // Still ready
        let mut writable = task::spawn(client.writable());
        assert_ready_ok!(writable.poll());

        match client.try_write(msg) {
            Ok(n) => written.extend(&msg[..n]),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => panic!("error = {:?}", e),
        }
    }

    {
        // Write buffer full
        let mut writable = task::spawn(client.writable());
        assert_pending!(writable.poll());

        // Drain the socket from the server end
        let mut read = vec![0; written.len()];
        let mut i = 0;

        while i < read.len() {
            server.readable().await?;

            match server.try_read(&mut read[i..]) {
                Ok(n) => i += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("error = {:?}", e),
            }
        }

        assert_eq!(read, written);
    }

    // Now, we listen for shutdown
    drop(client);

    loop {
        let ready = server.ready(Interest::READABLE).await?;

        if ready.is_read_closed() {
            break;
        } else {
            tokio::task::yield_now().await;
        }
    }

    Ok(())
}

async fn create_pair() -> (UnixStream, UnixStream) {
    let dir = assert_ok!(tempfile::tempdir());
    let bind_path = dir.path().join("bind.sock");

    let listener = assert_ok!(UnixListener::bind(&bind_path));

    let accept = listener.accept();
    let connect = UnixStream::connect(&bind_path);
    let ((server, _), client) = assert_ok!(try_join(accept, connect).await);

    (client, server)
}

macro_rules! assert_readable_by_polling {
    ($stream:expr) => {
        assert_ok!(poll_fn(|cx| $stream.poll_read_ready(cx)).await);
    };
}

macro_rules! assert_not_readable_by_polling {
    ($stream:expr) => {
        poll_fn(|cx| {
            assert_pending!($stream.poll_read_ready(cx));
            Poll::Ready(())
        })
        .await;
    };
}

macro_rules! assert_writable_by_polling {
    ($stream:expr) => {
        assert_ok!(poll_fn(|cx| $stream.poll_write_ready(cx)).await);
    };
}

macro_rules! assert_not_writable_by_polling {
    ($stream:expr) => {
        poll_fn(|cx| {
            assert_pending!($stream.poll_write_ready(cx));
            Poll::Ready(())
        })
        .await;
    };
}

#[tokio::test]
async fn poll_read_ready() {
    let (mut client, mut server) = create_pair().await;

    // Initial state - not readable.
    assert_not_readable_by_polling!(server);

    // There is data in the buffer - readable.
    assert_ok!(client.write_all(b"ping").await);
    assert_readable_by_polling!(server);

    // Readable until calls to `poll_read` return `Poll::Pending`.
    let mut buf = [0u8; 4];
    assert_ok!(server.read_exact(&mut buf).await);
    assert_readable_by_polling!(server);
    read_until_pending(&mut server);
    assert_not_readable_by_polling!(server);

    // Detect the client disconnect.
    drop(client);
    assert_readable_by_polling!(server);
}

#[tokio::test]
async fn poll_write_ready() {
    let (mut client, server) = create_pair().await;

    // Initial state - writable.
    assert_writable_by_polling!(client);

    // No space to write - not writable.
    write_until_pending(&mut client);
    assert_not_writable_by_polling!(client);

    // Detect the server disconnect.
    drop(server);
    assert_writable_by_polling!(client);
}

fn read_until_pending(stream: &mut UnixStream) {
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match stream.try_read(&mut buf) {
            Ok(_) => (),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }
}

fn write_until_pending(stream: &mut UnixStream) {
    let buf = vec![0u8; 1024 * 1024];
    loop {
        match stream.try_write(&buf) {
            Ok(_) => (),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }
}

#[tokio::test]
async fn try_read_buf() -> std::io::Result<()> {
    let msg = b"hello world";

    let dir = tempfile::tempdir()?;
    let bind_path = dir.path().join("bind.sock");

    // Create listener
    let listener = UnixListener::bind(&bind_path)?;

    // Create socket pair
    let client = UnixStream::connect(&bind_path).await?;

    let (server, _) = listener.accept().await?;
    let mut written = msg.to_vec();

    // Track the server receiving data
    let mut readable = task::spawn(server.readable());
    assert_pending!(readable.poll());

    // Write data.
    client.writable().await?;
    assert_eq!(msg.len(), client.try_write(msg)?);

    // The task should be notified
    while !readable.is_woken() {
        tokio::task::yield_now().await;
    }

    // Fill the write buffer
    loop {
        // Still ready
        let mut writable = task::spawn(client.writable());
        assert_ready_ok!(writable.poll());

        match client.try_write(msg) {
            Ok(n) => written.extend(&msg[..n]),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => panic!("error = {:?}", e),
        }
    }

    {
        // Write buffer full
        let mut writable = task::spawn(client.writable());
        assert_pending!(writable.poll());

        // Drain the socket from the server end
        let mut read = Vec::with_capacity(written.len());
        let mut i = 0;

        while i < read.capacity() {
            server.readable().await?;

            match server.try_read_buf(&mut read) {
                Ok(n) => i += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("error = {:?}", e),
            }
        }

        assert_eq!(read, written);
    }

    // Now, we listen for shutdown
    drop(client);

    loop {
        let ready = server.ready(Interest::READABLE).await?;

        if ready.is_read_closed() {
            break;
        } else {
            tokio::task::yield_now().await;
        }
    }

    Ok(())
}
