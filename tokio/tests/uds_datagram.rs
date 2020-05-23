#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]

use tokio::net::UnixDatagram;
use tokio::try_join;

use std::io;

async fn echo_server(mut socket: UnixDatagram) -> io::Result<()> {
    let mut recv_buf = vec![0u8; 1024];
    loop {
        let (len, peer_addr) = socket.recv_from(&mut recv_buf[..]).await?;
        if let Some(path) = peer_addr.as_pathname() {
            socket.send_to(&recv_buf[..len], path).await?;
        }
    }
}

#[tokio::test]
async fn echo() -> io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let server_path = dir.path().join("server.sock");
    let client_path = dir.path().join("client.sock");

    let server_socket = UnixDatagram::bind(server_path.clone())?;

    tokio::spawn(async move {
        if let Err(e) = echo_server(server_socket).await {
            eprintln!("Error in echo server: {}", e);
        }
    });

    {
        let mut socket = UnixDatagram::bind(&client_path).unwrap();
        socket.connect(server_path)?;
        socket.send(b"ECHO").await?;
        let mut recv_buf = [0u8; 16];
        let len = socket.recv(&mut recv_buf[..]).await?;
        assert_eq!(&recv_buf[..len], b"ECHO");
    }

    Ok(())
}

#[tokio::test]
async fn split() -> std::io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("split.sock");
    let socket = UnixDatagram::bind(path.clone())?;
    let (mut r, mut s) = socket.into_split();

    let msg = b"hello";
    let ((), ()) = try_join! {
        async {
            s.send_to(msg, path).await?;
            io::Result::Ok(())
        },
        async {
            let mut recv_buf = [0u8; 32];
            let (len, _) = r.recv_from(&mut recv_buf[..]).await?;
            assert_eq!(&recv_buf[..len], msg);
            Ok(())
        },
    }?;

    Ok(())
}

#[tokio::test]
async fn reunite() -> std::io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reunite.sock");
    let socket = UnixDatagram::bind(path)?;
    let (s, r) = socket.into_split();
    assert!(s.reunite(r).is_ok());
    Ok(())
}

#[tokio::test]
async fn reunite_error() -> std::io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reunit.sock");
    let dir = tempfile::tempdir().unwrap();
    let path1 = dir.path().join("reunit.sock");
    let socket = UnixDatagram::bind(path)?;
    let socket1 = UnixDatagram::bind(path1)?;

    let (s, _) = socket.into_split();
    let (_, r1) = socket1.into_split();
    assert!(s.reunite(r1).is_err());
    Ok(())
}
