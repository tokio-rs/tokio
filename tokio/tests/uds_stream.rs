#![cfg(feature = "full")]
#![warn(rust_2018_idioms)]
#![cfg(unix)]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use futures::future::try_join;

#[tokio::test]
async fn accept_read_write() -> std::io::Result<()> {
    let dir = tempfile::Builder::new()
        .prefix("tokio-uds-tests")
        .tempdir()
        .unwrap();
    let sock_path = dir.path().join("connect.sock");

    let mut listener = UnixListener::bind(&sock_path)?;

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

    let mut listener = UnixListener::bind(&sock_path)?;

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
